use std::iter;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use ratatui::style::{Style, Stylize as _};
use ratatui::{layout, text, widgets};

use super::{Context, HandleInput, LayerCommand, LayerTrait};
use crate::util::center_subrect;

const HELP_INFO: &[(&str, &[(&str, &str)])] = &[
    ("Help", &[("q", "Close this menu")]),
    (
        "Main",
        &[
            ("?", "Display this menu"),
            ("q", "Exit the application"),
            ("SPACE", "Pause data"),
            ("-", "Zoom out (0.5x)"),
            ("=", "Zoom in (2x)"),
            ("h", "Move viewport leftwards by 10%"),
            ("H", "Move viewport leftwards by 50%"),
            ("l", "Move viewport rightwards by 10%"),
            ("L", "Move viewport rightwards by 50%"),
            ("r", "Reset viewport to the full backlog range"),
            ("g", "Focus on legend legend"),
        ],
    ),
    (
        "Warnings",
        &[
            ("w", "Focus/defocus warnings"),
            ("j", "Scroll down"),
            ("k", "Scroll up"),
            ("z", "Zoom warnings"),
            ("SPACE", "Freeze warnings"),
        ],
    ),
    (
        "Legend",
        &[
            ("g", "Focus/defocus legend"),
            ("H", "Move window leftwards"),
            ("L", "Move window rightwards"),
            ("K", "Move window upwards"),
            ("J", "Move window downwards"),
            ("k", "Focus on the previous series"),
            ("j", "Focus on the next series"),
            ("c r", "Make series color more red"),
            ("c R", "Make series color less red"),
            ("c g", "Make series color more green"),
            ("c G", "Make series color less green"),
            ("c b", "Make series color more blue"),
            ("c B", "Make series color less blue"),
        ],
    ),
];

pub struct LayerHelp;

impl LayerTrait for LayerHelp {
    fn render(&mut self, _context: &mut Context, frame: &mut ratatui::Frame) {
        let rect = center_subrect(frame.size(), (7, 10));
        frame.render_widget(widgets::Clear, rect);
        frame.render_widget(
            widgets::Table::default()
                .rows(HELP_INFO.iter().copied().flat_map(|(section, keys)| {
                    iter::once(widgets::Row::new([text::Span::styled(
                        section,
                        Style::default().bold(),
                    )]))
                    .chain(keys.iter().copied().map(|(key, desc)| {
                        widgets::Row::new([
                            text::Span::styled(key, Style::default().cyan()),
                            text::Span::styled(desc, Style::default()),
                        ])
                    }))
                    .chain(iter::once(widgets::Row::new([""; 0])))
                }))
                .header(widgets::Row::new(["Key", "Description"]).bottom_margin(1))
                .widths([layout::Constraint::Length(16), layout::Constraint::Percentage(80)])
                .column_spacing(1)
                .block(
                    widgets::Block::default()
                        .title("Help")
                        .borders(widgets::Borders::all())
                        .border_style(Style::default().bold()),
                ),
            rect,
        );
    }

    fn handle_input(
        &mut self,
        _context: &mut Context,
        event: &Event,
        layer_cmds: &mut Vec<LayerCommand>,
        _frame_size: layout::Rect,
    ) -> Result<HandleInput> {
        Ok(match event {
            Event::Key(KeyEvent { code: event::KeyCode::Char('q'), .. }) => {
                layer_cmds.push(LayerCommand::Remove);
                HandleInput::Consumed
            }
            Event::Key(KeyEvent { code: event::KeyCode::Char('?'), .. }) => {
                // Do not allow opening multiple help layers
                HandleInput::Consumed
            }
            _ => HandleInput::Fallthru,
        })
    }
}
