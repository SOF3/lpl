use std::iter;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use ratatui::style::{Style, Stylize};
use ratatui::text::Text;
use ratatui::{layout, style, widgets};

use super::{Context, HandleInput, LayerCommand, LayerTrait};
use crate::util::{self, disp_float, AnchoredPosition, Gravity, SaturatingSubExt, SaturatingAddExt};

pub struct LayerLegend {
    position:      AnchoredPosition,
    layer_focused: bool,
    series_focus:  Option<String>,
    changing_color: bool,
    last_dim: (u16, u16),
}

impl Default for LayerLegend {
    fn default() -> Self {
        Self {
            position:      AnchoredPosition {
                anchor:     Gravity::TOP | Gravity::RIGHT,
                x_displace: 0,
                y_displace: 0,
            },
            layer_focused: false,
            series_focus:  None,
            changing_color: false,
            last_dim:      (0, 0),
        }
    }
}

impl LayerTrait for LayerLegend {
    fn render(&mut self, context: &mut Context, frame: &mut ratatui::Frame) {
        let Some(targets) = &context.current_targets else { return };
        let (rows, max_widths): (Vec<_>, [usize; 2]) = targets
            .iter()
            .filter_map(|target| {
                let [color_r, color_g, color_b] = target.color;

                let last_value = disp_float(target.points.iter().map(|&(_, y)| y).last()?, 4);
                let widths = [target.label.len(), last_value.len()];

                let mut base_style = Style::default();
                if self.series_focus.as_ref().is_some_and(|name| name == &target.label) {
                    base_style = base_style.underlined();
                }

                let row = widgets::Row::new([
                    Text::styled(
                        target.label.clone(),
                        base_style.fg(style::Color::Rgb(color_r, color_g, color_b)),
                    ),
                    Text::styled(last_value, base_style),
                ]);
                Some((row, widths))
            })
            .fold((Vec::new(), [0, 0]), |(mut rows, mut max_widths), (row, widths)| {
                rows.push(row);
                for (max_width, width) in iter::zip(&mut max_widths, widths) {
                    *max_width = width.max(*max_width);
                }
                (rows, max_widths)
            });

        let table_width = (max_widths[0] + max_widths[1] + 1) as u16 + 2;
        let table_height = rows.len() as u16 + 2;
        self.last_dim = (table_width, table_height);

        let rect = self.position.to_rect(
            table_width,
            table_height,
            frame.size().inner(&layout::Margin { horizontal: 5, vertical: 2 }),
        );

        let mut border_style = Style::default();
        if self.layer_focused {
            border_style = border_style.on_black();
        }

        frame.render_widget(
            widgets::Table::default()
                .rows(rows)
                .widths(max_widths.map(|width| layout::Constraint::Length(width as u16)))
                .column_spacing(1)
                .block(
                    widgets::Block::default()
                        .title("Legend")
                        .borders(widgets::Borders::all())
                        .border_style(border_style),
                ),
            rect,
        );
    }

    fn handle_input(
        &mut self,
        context: &mut Context,
        event: &Event,
        _layer_cmds: &mut Vec<LayerCommand>,
        frame_size: layout::Rect,
    ) -> Result<HandleInput> {
        if self.changing_color {
            if let &Event::Key(KeyEvent { code: event::KeyCode::Char(key @ ('r' | 'R' | 'g' | 'G' | 'b' | 'B')), .. }) = event {
                self.changing_color = false;

                let Some(name) = self.series_focus.as_deref() else {
                    context.warning_sender.send(String::from("Cannot change color code because no series is selected"));
                    return Ok(HandleInput::Consumed);
                };

                let color = context.cache.colors.get_mut(name).expect("existing series name should have corresponding color entry");
                match key {
                    'r' => color[0].saturating_add_assign(5),
                    'R' => color[0].saturating_sub_assign(5),
                    'g' => color[1].saturating_add_assign(5),
                    'G' => color[1].saturating_sub_assign(5),
                    'b' => color[2].saturating_add_assign(5),
                    'B' => color[2].saturating_sub_assign(5),
                    _ => unreachable!(),
                }

                return Ok(HandleInput::Consumed)
            }
        }

        Ok(match event {
            Event::Key(KeyEvent { code: event::KeyCode::Char('g'), .. }) => {
                self.layer_focused = !self.layer_focused;
                HandleInput::Consumed
            }
            _ if !self.layer_focused => HandleInput::Fallthru,
            &Event::Key(KeyEvent {
                code: event::KeyCode::Char(key @ ('H' | 'J' | 'K' | 'L')),
                ..
            }) => {
                let dir = match key {
                    'H' => util::Direction::Left,
                    'L' => util::Direction::Right,
                    'K' => util::Direction::Top,
                    'J' => util::Direction::Bottom,
                    _ => unreachable!(),
                };
                self.position.move_towards(dir, 5);
                self.position.anchor_by_nearest(self.last_dim.0, self.last_dim.1, frame_size);
                HandleInput::Consumed
            }
            &Event::Key(KeyEvent { code: event::KeyCode::Char('c'), .. }) => {
                self.changing_color = true;
                HandleInput::Consumed
            }
            &Event::Key(KeyEvent { code: event::KeyCode::Char(input @ ('j' | 'k')), .. }) => {
                let series_names: Vec<_> = context.cache.data.map.keys().collect();

                self.series_focus = if series_names.is_empty() {
                    None
                } else {
                    let new_index = match self.series_focus.as_deref() {
                        None => match input {
                            'j' => 0,
                            'k' => series_names.len() - 1,
                            _ => unreachable!(),
                        },
                        Some(key) => {
                            let current_index =
                                series_names.iter().position(|name| *name == key).unwrap_or(0);
                            match input {
                                'j' => (current_index + 1) % series_names.len(),
                                'k' => {
                                    (current_index + series_names.len() - 1) % series_names.len()
                                }
                                _ => unreachable!(),
                            }
                        }
                    };
                    series_names.get(new_index).map(|string| string.to_string())
                };

                HandleInput::Consumed
            }
            _ => HandleInput::Fallthru,
        })
    }
}
