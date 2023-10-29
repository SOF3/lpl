use std::collections::VecDeque;
use std::thread;
use std::time::{Duration, SystemTime};

use anyhow::{Context as _, Result};
use arcstr::ArcStr;
use crossterm::event::{self, Event};
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode};
use futures::channel::mpsc;
use futures::{select, FutureExt, StreamExt as _};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::{layout, Terminal};
use tokio_util::sync::CancellationToken;

use crate::input::Input;
use crate::util;

mod layer_chart;
use layer_chart::LayerChart;
mod layer_help;
use layer_help::LayerHelp;
mod layer_warn;
use layer_warn::LayerWarn;
mod data;
use data::Data;

#[derive(clap::Args)]
#[group(id = "UI")]
pub struct Options {
    /// Number of warnings to keep in backlog.
    #[arg(long, default_value_t = 1000)]
    warning_backlog_size:     usize,
    /// Duration in seconds to keep warnings visible after a new warning appears.
    #[arg(long, value_parser = |v: &str| v.parse::<f32>().map(Duration::from_secs_f32), default_value = "5")]
    warning_display_duration: Duration,

    /// Duration in seconds to retain data for.
    #[arg(long, value_parser = |v: &str| v.parse::<f32>().map(Duration::from_secs_f32), default_value = "60")]
    data_backlog_duration: Duration,
}

pub async fn run(options: Options, input: Input, cancel: CancellationToken) -> Result<()> {
    enable_raw_mode()?;
    let _raii = util::Finally(Some(((), |()| disable_raw_mode().context("disable raw mode"))));

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;

    crossterm::execute!(terminal.backend_mut(), terminal::EnterAlternateScreen)?;
    let result = main_loop(options, cancel, &mut terminal, input).await;
    crossterm::execute!(terminal.backend_mut(), terminal::LeaveAlternateScreen)
        .context("reset terminal")?;
    result?; // execute after resetting

    Ok(())
}

struct Context {
    options:  Options,
    cancel:   CancellationToken,
    warnings: VecDeque<(SystemTime, ArcStr)>,
    data:     Data,
}

#[portrait::make]
trait LayerTrait {
    fn render(&mut self, context: &mut Context, frame: &mut ratatui::Frame);

    #[portrait(derive_delegate(reduce = |_, _| unreachable!()))]
    fn handle_input(
        &mut self,
        context: &mut Context,
        event: &Event,
        layer_cmds: &mut Vec<LayerCommand>,
    ) -> Result<HandleInput>;
}

enum HandleInput {
    Consumed,
    Fallthru,
}

#[portrait::derive(LayerTrait with portrait::derive_delegate)]
enum Layer {
    Base(LayerChart),
    Warn(LayerWarn),
    Help(LayerHelp),
}

enum LayerCommand {
    Insert(Layer, usize),
    Remove,
}

async fn main_loop(
    options: Options,
    cancel: CancellationToken,
    terminal: &mut Terminal<impl Backend>,
    Input { mut input, warnings }: Input,
) -> Result<()> {
    let mut events = {
        let (send, recv) = mpsc::unbounded();
        consume_events(cancel.clone(), send);
        Some(recv)
    };

    let mut context = Context { options, cancel, warnings: VecDeque::new(), data: Data::default() };

    let mut warnings = Some(warnings);

    let mut layers = vec![Layer::Base(LayerChart::default()), Layer::Warn(LayerWarn::default())];
    let mut layer_cmds: Vec<LayerCommand> = Vec::new();

    loop {
        terminal.draw(|frame| {
            for layer in &mut layers {
                layer.render(&mut context, frame);
            }
        })?;

        select! {
            _ = context.cancel.cancelled().fuse() => return Ok(()),
            event = util::some_or_pending(&mut events).fuse() => {
                for i in (0..layers.len()).rev() {
                    let layer = layers.get_mut(i).unwrap();
                    let flow = layer.handle_input(&mut context, &event, &mut layer_cmds)?;

                    let mut removed = false;
                    for cmd in layer_cmds.drain(..) {
                        match cmd {
                            LayerCommand::Insert(new_layer, offset) => {
                                layers.insert(i + 1 + offset, new_layer);
                            },
                            LayerCommand::Remove => {
                                assert!(i > 0);
                                assert!(!removed, "cannot remove twice");
                                removed = true;
                                layers.remove(i);
                            }
                        }
                    }

                    if let HandleInput::Consumed = flow {
                        break;
                    }
                }
            },
            message = input.next() => {
                let Some(message) = message else { return Ok(()) };
                context.data.trim(SystemTime::now() - context.options.data_backlog_duration);
                context.data.push_message(message);
            },
            (warning_time, warning_msg) = util::some_or_pending(&mut warnings).fuse() => {
                if context.warnings.len() >= context.options.warning_backlog_size {
                    _ = context.warnings.pop_front();
                }
                context.warnings.push_back((warning_time, warning_msg.into()));
            }
        }
    }
}

fn consume_events(cancel: CancellationToken, ch: mpsc::UnboundedSender<Event>) {
    thread::spawn(move || {
        while !cancel.is_cancelled() {
            if let Ok(true) = event::poll(Duration::from_millis(100)) {
                if let Ok(event) = event::read() {
                    if ch.unbounded_send(event).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

fn center_subrect(rect: layout::Rect, ratio: (u16, u16)) -> layout::Rect {
    let center_x = (rect.left() + rect.right()) / 2;
    let center_y = (rect.top() + rect.bottom()) / 2;
    let new_width = rect.width * ratio.0 / ratio.1;
    let new_height = rect.height * ratio.0 / ratio.1;

    let new_left = center_x - new_width / 2;
    let new_top = center_y - new_height / 2;

    layout::Rect { x: new_left, y: new_top, width: new_width, height: new_height }
}

bitflags::bitflags! {
    pub struct Gravity: u8 {
        const LEFT = 0;
        const RIGHT = 1;
        const TOP = 0;
        const BOTTOM = 2;
    }
}

pub fn rect_resize(
    mut rect: layout::Rect,
    gravity: Gravity,
    mut width: u16,
    mut height: u16,
) -> layout::Rect {
    width = width.min(rect.width);
    height = height.min(rect.height);

    if gravity.contains(Gravity::RIGHT) {
        rect.x = rect.right() - width;
    }
    if gravity.contains(Gravity::BOTTOM) {
        rect.y = rect.bottom() - height;
    }

    layout::Rect { width, height, ..rect }
}
