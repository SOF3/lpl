use std::ops;
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

use super::data::{Cache, Freezable};
use super::layer_help::LayerHelp;
use super::{Context, HandleInput, Layer, LayerCommand, LayerTrait, Options};

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
    data:   Freezable,
}

#[derive(Clone, Copy)]
struct RenderTimeRange {
    now:         SystemTime,
    since_start: Duration,
    since_end:   Duration,
}

struct DrawImpl<'t> {
    time:    RenderTimeRange,
    targets: &'t [DrawTarget],
}

impl RenderTimeRange {
    fn starts_at(&self) -> SystemTime { self.now - self.since_start }
    fn ends_at(&self) -> SystemTime { self.now - self.since_end }
    fn abs_range(&self) -> ops::RangeInclusive<SystemTime> { self.starts_at()..=self.ends_at() }
    fn neg_secs_range(&self) -> ops::Range<f64> {
        (-self.since_start.as_secs_f64())..(-self.since_end.as_secs_f64())
    }
    fn secs_to_abs(&self, secs: f64) -> SystemTime { self.now - Duration::from_secs_f64(-secs) }
}

pub(super) struct DrawTarget {
    pub(super) points:  Vec<(f64, f64)>,
    pub(super) visible: bool,
    pub(super) color:   [u8; 3],
    pub(super) label:   String,
}

fn data_to_targets(cache: &Cache, data: &Freezable, time: RenderTimeRange) -> Vec<DrawTarget> {
    data.map
        .iter()
        .map(|(label, series)| {
            (
                cache.disp_config.get(label).expect("series does not have corresponding color"),
                label,
                series.data.iter().filter(|datum| time.abs_range().contains(&datum.time)),
            )
        })
        .map(|(disp, label, series)| {
            let points = series
                .map(|datum| {
                    let x = time
                        .now
                        .duration_since(datum.time)
                        .expect("time should be in the past")
                        .as_secs_f64();
                    let y = datum.value;
                    (-x, y)
                })
                .collect();
            DrawTarget { points, visible: disp.visible, color: disp.color, label: label.clone() }
        })
        .collect()
}

impl Draw for DrawImpl<'_> {
    fn draw(&self, area: DrawingArea<RatatuiBackend, coord::Shift>) -> AreaResult {
        let global_y_extrema = self
            .targets
            .iter()
            .flat_map(|target| &target.points)
            .map(|&(_, y)| y)
            .fold(None::<(f64, f64)>, |extrema, y| {
                let (min, max) = extrema.unwrap_or((y, y));
                Some((min.min(y), max.max(y)))
            })
            .unwrap_or((0.0, 1.0));

        let x_range = self.time.neg_secs_range();
        let y_range = global_y_extrema.0..global_y_extrema.1;

        let mut chart = ChartBuilder::on(&area)
            .margin_left(24)
            .margin_bottom(12)
            .set_left_and_bottom_label_area_size(1)
            .build_cartesian_2d(x_range, y_range)?;

        for &DrawTarget { ref points, visible, color: [color_r, color_g, color_b], .. } in
            self.targets
        {
            if visible {
                chart.draw_series(LineSeries::new(
                    points.iter().copied(),
                    RGBColor(color_r, color_g, color_b),
                ))?;
            }
        }

        chart
            .configure_mesh()
            .disable_mesh()
            .axis_style(WHITE)
            .label_style(("", CHAR_PIXEL_SIZE).with_color(WHITE))
            .x_label_formatter(&|&value| {
                DateTime::<chrono::Local>::from(self.time.secs_to_abs(value))
                    .format("%H:%M:%S")
                    .to_string()
            })
            .draw()?;

        Ok(())
    }
}

impl LayerTrait for LayerChart {
    #[allow(clippy::cast_sign_loss)]
    fn render(&mut self, context: &mut Context, frame: &mut ratatui::Frame) {
        const SCROLL_DENOMINATOR: usize = 1000;

        let (now, data) = if let Some(freeze) = &self.freeze {
            (freeze.frozen, &freeze.data)
        } else {
            context.cache.trim(SystemTime::now() - context.options.data_backlog_duration);
            (SystemTime::now(), &context.cache.data)
        };

        let time = RenderTimeRange { now, since_start: self.x_start, since_end: self.x_end };
        let targets = &*context.current_targets.insert(data_to_targets(&context.cache, data, time));

        let chart = PlottersWidget {
            draw:          DrawImpl { time, targets },
            error_handler: |err| {
                context.warning_sender.clone().send(format!("Plotting error: {err:?}"));
            },
        };
        let rect = frame.area();
        frame.render_widget(chart, rect.inner(layout::Margin { vertical: 1, horizontal: 0 }));

        let x_start_display = self.x_start.min(context.options.data_backlog_duration);
        let x_midpt_display = ((x_start_display + self.x_end) / 2).as_secs_f64();
        let x_interval_display = (x_start_display - self.x_end).as_secs_f64();
        let scroll_interval_ratio =
            x_interval_display / context.options.data_backlog_duration.as_secs_f64();
        let scroll_midpt_ratio = (context.options.data_backlog_duration.as_secs_f64()
            - x_midpt_display)
            / context.options.data_backlog_duration.as_secs_f64();
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
        _frame_size: layout::Rect,
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
                        data:   context.cache.data.clone(),
                    })),
                };
                HandleInput::Consumed
            }
            Event::Key(KeyEvent {
                code: event::KeyCode::Char(key @ ('-' | '=' | 'h' | 'l' | 'H' | 'L')),
                ..
            }) => {
                #[allow(clippy::type_complexity)]
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
                let end = new_midpt
                    .saturating_sub(right_semiitv)
                    .min(context.options.data_backlog_duration);
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
