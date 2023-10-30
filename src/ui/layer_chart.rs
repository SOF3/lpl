use std::time::{Duration, SystemTime};

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use ratatui::style::{Style, Stylize as _};
use ratatui::{layout, text, widgets};

use super::data::Data;
use super::layer_help::LayerHelp;
use super::{Context, HandleInput, Layer, LayerCommand, LayerTrait, Options};

const DEFAULT_PALETTE: &[fn(Style) -> Style] = &[
    Style::white,
    Style::light_cyan,
    Style::light_magenta,
    Style::light_blue,
    Style::light_yellow,
    Style::light_green,
    Style::light_red,
    Style::dark_gray,
    Style::gray,
    Style::cyan,
    Style::magenta,
    Style::blue,
    Style::yellow,
    Style::green,
    Style::red,
    Style::black,
];

pub struct LayerChart {
    freeze: Option<Box<Freeze>>,

    x_start: Duration,
    x_end:   Duration,
}

impl LayerChart {
    pub fn new(options: &Options) -> Self {
        Self { freeze: None, x_start: options.data_backlog_duration, x_end: Duration::ZERO }
    }
}

struct Freeze {
    frozen: SystemTime,
    data:   Data,
}

impl LayerTrait for LayerChart {
    fn render(&mut self, context: &mut Context, frame: &mut ratatui::Frame) {
        let (now, data) = match &self.freeze {
            Some(freeze) => (freeze.frozen, &freeze.data),
            None => {
                context.data.trim(SystemTime::now() - context.options.data_backlog_duration);
                (SystemTime::now(), &context.data)
            }
        };

        let x_start = now - self.x_start;
        let x_end = now - self.x_end;
        let x_interval = self.x_start - self.x_end;

        let data_vecs: Vec<_> = data
            .series_map
            .iter()
            .filter_map(|(label, series)| {
                let mut data: Vec<_> = series
                    .data
                    .iter()
                    .filter(|datum| (x_start..=x_end).contains(&datum.time))
                    .map(|datum| {
                        let x = datum
                            .time
                            .duration_since(x_start)
                            .expect("time contained in x_start..=x_end")
                            .as_secs_f64();
                        let y = datum.value;
                        (x, y)
                    })
                    .collect();
                if let Some(&(_, y)) = data.last() {
                    data.push((x_interval.as_secs_f64(), y));
                    Some((format!("{label}: {}", disp_float(y, 4)), data))
                } else {
                    None
                }
            })
            .collect();
        let datasets = data_vecs
            .iter()
            .filter(|(_, data)| !data.is_empty())
            .enumerate()
            .map(|(i, (label, data))| {
                let style = DEFAULT_PALETTE[i % DEFAULT_PALETTE.len()](Style::default());

                widgets::Dataset::default()
                    .name(label)
                    .data(data)
                    .graph_type(widgets::GraphType::Line)
                    .marker(ratatui::symbols::Marker::Braille)
                    .style(style)
            })
            .collect();

        let (y_min, y_max) = data_vecs
            .iter()
            .flat_map(|(_, data)| data.iter().map(|&(_, y)| y))
            .fold(None, |state, y| {
                let Some((min, max)) = state else { return Some((y, y)) };
                Some((min.min(y), max.max(y)))
            })
            .unwrap_or((0.0, 1.0));

        let rect = frame.size();
        let chart = widgets::Chart::new(datasets)
            .x_axis(
                widgets::Axis::default()
                    .title("Time")
                    .bounds([0.0, x_interval.as_secs_f64()])
                    .labels(
                        lerp_iter(-self.x_start.as_secs_f64(), -self.x_end.as_secs_f64(), 3)
                            .map(|x| text::Span::raw(disp_float(x, 4)))
                            .collect(),
                    )
                    .style(Style::default().white()),
            )
            .y_axis(
                widgets::Axis::default()
                    .title("Value")
                    .bounds([y_min, y_max])
                    .labels(
                        lerp_iter(y_min, y_max, 3)
                            .map(|y| text::Span::raw(disp_float(y, 4)))
                            .collect(),
                    )
                    .style(Style::default().white()),
            )
            .hidden_legend_constraints((
                layout::Constraint::Percentage(100),
                layout::Constraint::Percentage(100),
            ));
        frame.render_widget(chart, rect.inner(&layout::Margin { vertical: 1, horizontal: 0 }));

        let x_start_display = self.x_start.min(context.options.data_backlog_duration);
        let x_midpt_display = ((x_start_display + self.x_end) / 2).as_secs_f64();
        let x_interval_display = (x_start_display - self.x_end).as_secs_f64();
        let scroll_interval_ratio =
            x_interval_display / context.options.data_backlog_duration.as_secs_f64();
        let scroll_midpt_ratio = (context.options.data_backlog_duration.as_secs_f64()
            - x_midpt_display)
            / context.options.data_backlog_duration.as_secs_f64();
        const SCROLL_DENOMINATOR: usize = 1000;
        let scroll_size = ((SCROLL_DENOMINATOR as f64) * scroll_interval_ratio) as usize;
        let scroll_position = ((SCROLL_DENOMINATOR as f64)
            * unlerp(
                scroll_interval_ratio / 2.,
                1. - scroll_interval_ratio / 2.,
                scroll_midpt_ratio,
            )) as usize;
        let mut state = widgets::ScrollbarState::new(SCROLL_DENOMINATOR)
            .position(scroll_position)
            .viewport_content_length(scroll_size);

        let mut begin_style = Style::default();
        if self.x_start > context.options.data_backlog_duration {
            begin_style = begin_style.light_red();
        }

        frame.render_stateful_widget(
            widgets::Scrollbar::new(widgets::ScrollbarOrientation::HorizontalBottom)
                .begin_style(begin_style),
            rect,
            &mut state,
        );
    }

    fn handle_input(
        &mut self,
        context: &mut Context,
        event: &Event,
        layer_cmds: &mut Vec<LayerCommand>,
    ) -> Result<HandleInput> {
        Ok(match event {
            Event::Key(KeyEvent { code: event::KeyCode::Char('q'), .. }) => {
                context.cancel.cancel();
                HandleInput::Consumed
            }
            Event::Key(KeyEvent { code: event::KeyCode::Char('?'), .. }) => {
                layer_cmds.push(LayerCommand::Insert(Layer::Help(LayerHelp), 1));
                HandleInput::Consumed
            }
            Event::Key(KeyEvent { code: event::KeyCode::Char(' '), .. }) => {
                self.freeze = match self.freeze {
                    Some(_) => None,
                    None => Some(Box::new(Freeze {
                        frozen: SystemTime::now(),
                        data:   context.data.clone(),
                    })),
                };
                HandleInput::Consumed
            }
            Event::Key(KeyEvent {
                code: event::KeyCode::Char(key @ ('-' | '=' | 'h' | 'l' | 'H' | 'L')),
                ..
            }) => {
                let (itv_fn, midpt_fn): (
                    fn(Duration) -> Duration,
                    fn(Duration, Duration) -> Duration,
                ) = match key {
                    '-' => (|itv| itv * 2, |midpt, _| midpt),
                    '=' => (|itv| itv / 2, |midpt, _| midpt),
                    'h' => (|itv| itv, |midpt, itv| midpt + itv / 10),
                    'l' => (|itv| itv, |midpt, itv| midpt.saturating_sub(itv / 10)),
                    'H' => (|itv| itv, |midpt, itv| midpt + itv / 2),
                    'L' => (|itv| itv, |midpt, itv| midpt.saturating_sub(itv / 2)),
                    _ => unreachable!(),
                };

                let midpt = (self.x_start + self.x_end) / 2;

                let left_semiitv = itv_fn(self.x_start - midpt);
                let right_semiitv = itv_fn(midpt - self.x_end);
                let new_midpt = midpt_fn(midpt, self.x_start - self.x_end);

                let start =
                    (new_midpt + left_semiitv).min(context.options.data_backlog_duration * 2);
                let end = new_midpt.saturating_sub(right_semiitv);
                (self.x_start, self.x_end) = (start, end);
                HandleInput::Consumed
            }
            Event::Key(KeyEvent { code: event::KeyCode::Char('r'), .. }) => {
                self.x_start = context.options.data_backlog_duration;
                self.x_end = Duration::ZERO;
                HandleInput::Consumed
            }
            _ => HandleInput::Fallthru,
        })
    }
}

fn lerp(a: f64, b: f64, ratio: f64) -> f64 { a + (b - a) * ratio }

fn unlerp(a: f64, b: f64, v: f64) -> f64 { (v - a) / (b - a) }

fn lerp_iter(a: f64, b: f64, steps: u16) -> impl Iterator<Item = f64> {
    (0..steps)
        .map(move |step| f64::from(step) / f64::from(steps - 1))
        .map(move |ratio| lerp(a, b, ratio))
}

fn disp_float(value: f64, digits: u32) -> String {
    if value == 0.0 {
        return String::from("0");
    }

    let log = value.abs().log10().floor() as i32;

    let sig_base = 10f64.powi(log - (digits as i32));
    let rounded = (value / sig_base).round();

    if log.abs() >= digits as i32 {
        format!("{:.*}e{log}", digits as usize, rounded * 10f64.powi(-(digits as i32)))
    } else {
        format!("{:.*}", digits as usize, rounded * sig_base)
    }
}
