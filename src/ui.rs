use std::collections::VecDeque;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context as _, Result};
use arcstr::ArcStr;
use crossterm::event::{self, Event};
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode};
use futures::channel::mpsc;
use futures::{select, FutureExt, StreamExt as _};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::{layout, Terminal};
use tokio::time;
use tokio_util::sync::CancellationToken;

use crate::input::{Input, WarningSender};
use crate::util;

mod layer_chart;
use layer_chart::LayerChart;
mod layer_help;
use layer_help::LayerHelp;
mod layer_legend;
use layer_legend::LayerLegend;
mod layer_warn;
use layer_warn::LayerWarn;
mod data;
use data::Cache;

#[derive(Debug, clap::Args)]
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
    options:         Options,
    cancel:          CancellationToken,
    warnings:        VecDeque<(SystemTime, ArcStr)>,
    warning_sender:  WarningSender,
    cache:           Cache,
    current_targets: Option<Vec<layer_chart::DrawTarget>>,
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
        frame_size: layout::Rect,
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
    Legend(LayerLegend),
}

enum LayerCommand {
    Insert(Layer, usize),
    Remove,
}

async fn main_loop(
    options: Options,
    cancel: CancellationToken,
    terminal: &mut Terminal<impl Backend>,
    Input { messages: mut input, warnings, warning_sender }: Input,
) -> Result<()> {
    let mut events = {
        let (send, recv) = mpsc::unbounded();
        consume_events(cancel.clone(), send);
        Some(recv)
    };

    let mut context = Context {
        options,
        cancel,
        warnings: VecDeque::new(),
        warning_sender,
        cache: Cache::default(),
        current_targets: None,
    };

    let mut warnings = Some(warnings);

    let mut layers = vec![
        Layer::Base(LayerChart::new(&context.options)),
        Layer::Legend(LayerLegend::default()),
        Layer::Warn(LayerWarn::default()),
    ];
    let mut layer_cmds: Vec<LayerCommand> = Vec::new();

    let redraw_freq = Duration::from_millis(200);
    let mut redraw = true;
    let mut last_message_redraw = Instant::now();
    let mut last_area = None;

    loop {
        if redraw {
            let frame = terminal.draw(|frame| {
                for layer in &mut layers {
                    layer.render(&mut context, frame);
                }
            })?;
            last_area = Some(frame.area);
        }

        redraw = select! {
            () = context.cancel.cancelled().fuse() => return Ok(()),
            event = util::some_or_pending(&mut events).fuse() => {
                for i in (0..layers.len()).rev() {
                    let layer = layers.get_mut(i).unwrap();
                    let flow = layer.handle_input(&mut context, &event, &mut layer_cmds, last_area.expect("redraw was true the first time"))?;

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
                true
            },
            message = input.next() => {
                let Some(message) = message else { return Ok(()) };
                context.cache.trim(SystemTime::now() - context.options.data_backlog_duration);
                context.cache.push_message(message);

                if last_message_redraw.elapsed() < redraw_freq {
                    false
                } else {
                    last_message_redraw = Instant::now();
                    true
                }
            },
            (warning_time, warning_msg) = util::some_or_pending(&mut warnings).fuse() => {
                if context.warnings.len() >= context.options.warning_backlog_size {
                    _ = context.warnings.pop_front();
                }
                context.warnings.push_back((warning_time, warning_msg.into()));

                if last_message_redraw.elapsed() < redraw_freq {
                    false
                } else {
                    last_message_redraw = Instant::now();
                    true
                }
            },
            () = time::sleep(redraw_freq).fuse() => true // ensure redraw as time elapses
        };
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
