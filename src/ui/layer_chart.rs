use std::time::SystemTime;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use ordered_float::OrderedFloat;
use ratatui::style::{Style, Stylize as _};
use ratatui::{layout, text, widgets};

use super::data::Data;
use super::layer_help::LayerHelp;
use super::{Context, HandleInput, Layer, LayerCommand, LayerTrait};

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

#[derive(Default)]
pub struct LayerChart {
    freeze: Option<Box<Freeze>>,
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

        let data_vecs: Vec<_> = data
            .series_map
            .iter()
            .map(|(label, series)| {
                let data: Vec<_> = series
                    .data
                    .iter()
                    .map(|datum| {
                        let x = -now.duration_since(datum.time).unwrap_or_default().as_secs_f64();
                        let y = datum.value;
                        (x, y)
                    })
                    .collect();
                (label.clone(), data)
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

        let x_min = data_vecs
            .iter()
            .flat_map(|(_, data)| data.iter().map(|&(x, _)| OrderedFloat(x)))
            .min()
            .map(|x| x.0)
            .unwrap_or(-1.0);
        let (y_min, y_max) = data_vecs
            .iter()
            .flat_map(|(_, data)| data.iter().map(|&(_, y)| y))
            .fold(None, |state, y| {
                let Some((min, max)) = state else { return Some((y, y)) };
                Some((min.min(y), max.max(y)))
            })
            .unwrap_or((0.0, 1.0));

        let rect = frame.size().inner(&layout::Margin::new(5, 5));
        let chart = widgets::Chart::new(datasets)
            .x_axis(
                widgets::Axis::default()
                    .title("Time")
                    .bounds([x_min, 0.0])
                    .labels(
                        lerp_iter(x_min, 0.0, 3)
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
            );
        frame.render_widget(chart, rect)
    }

    fn handle_input(
        &mut self,
        context: &mut Context,
        event: &Event,
        layer_cmds: &mut Vec<LayerCommand>,
    ) -> Result<HandleInput> {
        match event {
            Event::Key(KeyEvent { code: event::KeyCode::Char('q'), .. }) => {
                context.cancel.cancel();
                return Ok(HandleInput::Consumed);
            }
            Event::Key(KeyEvent { code: event::KeyCode::Char('?'), .. }) => {
                layer_cmds.push(LayerCommand::Insert(Layer::Help(LayerHelp), 1));
                return Ok(HandleInput::Consumed);
            }
            Event::Key(KeyEvent { code: event::KeyCode::Char(' '), .. }) => {
                self.freeze = match self.freeze {
                    Some(_) => None,
                    None => Some(Box::new(Freeze {
                        frozen: SystemTime::now(),
                        data:   context.data.clone(),
                    })),
                };
                return Ok(HandleInput::Consumed);
            }
            _ => {}
        }
        Ok(HandleInput::Fallthru)
    }
}

fn lerp(a: f64, b: f64, ratio: f64) -> f64 { a + (b - a) * ratio }

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
