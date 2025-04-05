use std::collections::VecDeque;
use std::time::SystemTime;

use anyhow::Result;
use arcstr::ArcStr;
use crossterm::event::{self, Event, KeyEvent};
use ratatui::style::{Style, Stylize as _};
use ratatui::{layout, text, widgets};

use super::{Context, HandleInput, LayerCommand, LayerTrait};
use crate::util::{center_subrect, rect_resize, Gravity};

#[derive(Default)]
pub struct LayerWarn {
    focused: bool,
    zoomed:  bool,
    freeze:  Option<VecDeque<(SystemTime, ArcStr)>>,
    offset:  usize,
}

impl LayerWarn {
    fn is_visible(&self, context: &mut Context) -> VisibleReason {
        let mut output = VisibleReason::empty();

        if self.focused {
            output |= VisibleReason::FOCUSED;
        }

        if let Some(&(time, _)) = context.warnings.back() {
            if time + context.options.warning_display_duration > SystemTime::now() {
                output |= VisibleReason::RECENT_WARNING;
            }
        }

        output
    }

    fn warnings_src<'t>(&'t self, context: &'t Context) -> &'t VecDeque<(SystemTime, ArcStr)> {
        self.freeze.as_ref().unwrap_or(&context.warnings)
    }
}

bitflags::bitflags! {
    struct VisibleReason: u8 {
        const FOCUSED = 1;
        const RECENT_WARNING = 2;
    }
}

impl LayerTrait for LayerWarn {
    fn render(&mut self, context: &mut Context, frame: &mut ratatui::Frame) {
        const DISPLAYED_ITEMS: usize = 16;

        let visible = self.is_visible(context);

        if !visible.is_empty() {
            let src = self.warnings_src(context);
            let mut text: Vec<_> = src
                .iter()
                .rev()
                .skip(self.offset)
                .take(DISPLAYED_ITEMS)
                .rev()
                .flat_map(|&(time, ref message)| {
                    message.trim_end().split('\n').enumerate().map(move |(i, line)| {
                        text::Line::from(
                            [
                                if i == 0 {
                                    text::Span::styled(
                                        chrono::DateTime::<chrono::offset::Local>::from(time)
                                            .format("%H:%M:%S%.3f")
                                            .to_string(),
                                        Style::default().cyan(),
                                    )
                                } else {
                                    text::Span::raw(std::str::from_utf8(&[b' '; 12]).unwrap())
                                },
                                text::Span::raw(format!(" {line}")),
                            ]
                            .to_vec(),
                        )
                    })
                })
                .collect();

            if text.is_empty() {
                text.push(text::Line::styled("No warnings", Style::default().dim()));
            }

            let mut border_style = Style::default().yellow();
            if visible.contains(VisibleReason::RECENT_WARNING) {
                border_style = border_style.rapid_blink();
            }
            if visible.contains(VisibleReason::FOCUSED) {
                border_style = border_style.on_black();
            }

            let mut title = vec![text::Span::raw("Warnings")];

            let scroll_pos = src.len().saturating_sub(self.offset);
            let scroll_size = src.len();

            if self.offset > 0 {
                title.push(text::Span::styled(
                    format!(" [{scroll_pos}/{scroll_size}]"),
                    Style::default().light_blue(),
                ));
            }
            if self.freeze.is_some() {
                title.push(text::Span::styled(" [FROZEN]", Style::default().red()));
            }

            let rect = if self.zoomed {
                center_subrect(frame.area(), (8, 10))
            } else {
                warn_rect(
                    frame.area(),
                    text.len(),
                    text.iter().map(ratatui::text::Line::width).max().unwrap_or(0),
                )
            };

            frame.render_widget(
                widgets::Paragraph::new(text).block(
                    widgets::Block::default()
                        .title(title)
                        .borders(widgets::Borders::all())
                        .border_style(border_style),
                ),
                rect,
            );

            let mut state = widgets::ScrollbarState::new(scroll_size).position(scroll_pos);
            frame.render_stateful_widget(
                widgets::Scrollbar::new(widgets::ScrollbarOrientation::VerticalRight),
                rect.inner(layout::Margin { vertical: 1, horizontal: 0 }),
                &mut state,
            );
        }
    }

    fn handle_input(
        &mut self,
        context: &mut Context,
        event: &Event,
        _layer_cmds: &mut Vec<LayerCommand>,
        _frame_size: layout::Rect,
    ) -> Result<HandleInput> {
        Ok(match event {
            Event::Key(KeyEvent { code: event::KeyCode::Char('w'), .. }) => {
                self.focused = !self.focused;
                self.zoomed = false;
                self.freeze = None;

                HandleInput::Consumed
            }
            &Event::Key(KeyEvent { code: event::KeyCode::Char('z'), .. }) if self.focused => {
                self.zoomed = !self.zoomed;
                HandleInput::Consumed
            }
            &Event::Key(KeyEvent { code: event::KeyCode::Char(' '), .. }) if self.focused => {
                if self.freeze.is_some() {
                    self.freeze = None;
                } else {
                    self.freeze = Some(context.warnings.clone());
                }
                HandleInput::Consumed
            }
            &Event::Key(KeyEvent {
                code: event::KeyCode::Char(key @ ('j' | 'k' | 'g' | 'G')),
                ..
            }) if self.focused => {
                let max_offset = self.warnings_src(context).len().saturating_sub(1);
                self.offset = match key {
                    'j' => self.offset.saturating_sub(1),
                    'k' => self.offset.saturating_add(1).min(max_offset),
                    'g' => max_offset,
                    'G' => 0,
                    _ => unreachable!(),
                };
                HandleInput::Consumed
            }
            _ => HandleInput::Fallthru,
        })
    }
}

fn warn_rect(rect: layout::Rect, display_size: usize, max_width: usize) -> layout::Rect {
    let inner = rect.inner(layout::Margin { vertical: 3, horizontal: 5 });
    rect_resize(
        inner,
        Gravity::TOP | Gravity::RIGHT,
        50.max(inner.width * 4 / 10)
            .min(u16::try_from(max_width).unwrap_or(u16::MAX).saturating_add(2)),
        u16::try_from(display_size).unwrap_or(u16::MAX).saturating_add(2),
    )
}
