use std::time::{Duration, SystemTime};

use anyhow::Result;
use chrono::DateTime;
use crossterm::event::{self, Event, KeyEvent};
use plotters::coord;
use plotters::prelude::{ChartBuilder, DrawingArea};
use plotters::series::LineSeries;
use plotters::style::{IntoTextStyle, RGBColor, WHITE};
use plotters_ratatui_backend::{AreaResult, Draw, PlottersWidget, RatatuiBackend, CHAR_PIXEL_SIZE};
use ratatui::style::{Style, Stylize as _};
use ratatui::{layout, widgets};

use super::data::{Data, Datum};
use super::layer_help::LayerHelp;
use super::{Context, HandleInput, Layer, LayerCommand, LayerTrait, Options};

fn legend_label_size() -> u32 { ((CHAR_PIXEL_SIZE as f64) / 1.25).round() as u32 }

const DEFAULT_COLOR_MAP: &[RGBColor] = &[
    // Source: Set1 from matplotlib
    RGBColor(228, 26, 28),
    RGBColor(55, 126, 184),
    RGBColor(77, 175, 74),
    RGBColor(152, 78, 163),
    RGBColor(255, 127, 0),
    RGBColor(255, 255, 51),
    RGBColor(166, 86, 40),
    RGBColor(247, 129, 191),
    RGBColor(153, 153, 153),
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

struct DrawImpl<'t> {
    now:     SystemTime,
    x_start: Duration,
    x_end:   Duration,
    data:    &'t Data,
}

impl<'t> Draw for DrawImpl<'t> {
    fn draw(&self, area: DrawingArea<RatatuiBackend, coord::Shift>) -> AreaResult {
        struct ToDraw<PointsIter: Iterator<Item = (f64, f64)>> {
            points:     PointsIter,
            color:      RGBColor,
            disp_label: String,
        }

        let x_start_at = self.now - self.x_start;
        let x_end_at = self.now - self.x_end;

        let mut global_y_extrema = None::<(f64, f64)>;
        let mut to_draw = Vec::new();

        for (i, (label, series)) in self.data.series_map.iter().enumerate() {
            let series =
                series.data.iter().filter(|datum| (x_start_at..=x_end_at).contains(&datum.time));
            let Some(&Datum { value: last_y, .. }) = series.clone().next_back() else { continue };

            let (y_min, y_max) = series
                .clone()
                .fold(None::<(f64, f64)>, |state, &Datum { value: y, .. }| {
                    let Some((min, max)) = state else { return Some((y, y)) };
                    Some((min.min(y), max.max(y)))
                })
                .unwrap_or((0.0, 1.0));
            {
                let (global_y_min, global_y_max) =
                    &mut global_y_extrema.get_or_insert((y_min, y_max));
                *global_y_min = (*global_y_min).min(y_min);
                *global_y_max = (*global_y_max).max(y_max);
            }

            let points = series.map(|datum| {
                let x = self
                    .now
                    .duration_since(datum.time)
                    .expect("time should be in the past")
                    .as_secs_f64();
                let y = datum.value;
                (-x, y)
            });
            let color = DEFAULT_COLOR_MAP[i % DEFAULT_COLOR_MAP.len()];
            let disp_label = format!("{label}: {}", disp_float(last_y, 4));
            to_draw.push(ToDraw { points, color, disp_label });
        }

        let x_range = (-self.x_start.as_secs_f64())..(-self.x_end.as_secs_f64());
        let y_range = global_y_extrema.map_or(0.0..1.0, |(min, max)| min..max);

        let max_legend_len =
            to_draw.iter().map(|series| series.disp_label.len()).max().unwrap_or(0);

        let mut chart = ChartBuilder::on(&area)
            .margin_left(24)
            .margin_bottom(12)
            .set_left_and_bottom_label_area_size(1)
            .build_cartesian_2d(x_range, y_range)?;

        for ToDraw { points, color, disp_label } in to_draw {
            chart.draw_series(LineSeries::new(points, color))?.label(disp_label).legend(
                move |(x, y)| plotters::element::PathElement::new([(x, y), (x + 10, y)], color),
            );
        }

        chart
            .configure_mesh()
            .disable_mesh()
            .axis_style(WHITE)
            .label_style(("", CHAR_PIXEL_SIZE).with_color(WHITE))
            .x_label_formatter(&|&value| {
                DateTime::<chrono::Local>::from(self.now - Duration::from_secs_f64(-value))
                    .format("%H:%M:%S")
                    .to_string()
            })
            .draw()?;
        chart
            .configure_series_labels()
            .border_style(WHITE)
            .label_font(("", legend_label_size()).with_color(WHITE))
            .legend_area_size(CHAR_PIXEL_SIZE * (max_legend_len + 2) as u32)
            .draw()?;

        Ok(())
    }
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

        let chart = PlottersWidget {
            draw:          DrawImpl { now, x_start: self.x_start, x_end: self.x_end, data },
            error_handler: |err| {
                context.warning_sender.clone().send(format!("Plotting error: {err:?}"))
            },
        };
        let rect = frame.size();
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
                    '-' => (|itv| itv * 5 / 4, |midpt, _| midpt),
                    '=' => (|itv| itv * 4 / 5, |midpt, _| midpt),
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

fn unlerp(a: f64, b: f64, v: f64) -> f64 { (v - a) / (b - a) }

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
