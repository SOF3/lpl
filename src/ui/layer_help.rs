use std::iter;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use ratatui::style::{Style, Stylize as _};
use ratatui::{layout, text, widgets};

use super::{center_subrect, Context, HandleInput, LayerCommand, LayerTrait};

const HELP_INFO: &[(&str, &[(&str, &str)])] = &[
    (
        "Main",
        &[
            ("?", "Display this menu"),
            ("q", "Exit the application"),
            ("w", "Focus warnings"),
            ("SPACE", "Pause data"),
        ],
    ),
    (
        "Warnings",
        &[
            ("w", "Defocus warnings"),
            ("j", "Scroll down"),
            ("k", "Scroll up"),
            ("z", "Zoom warnings"),
            ("SPACE", "Freeze warnings"),
        ],
    ),
    ("Help", &[("q", "Close this menu")]),
];

pub struct LayerHelp;

impl LayerTrait for LayerHelp {
    fn render(&mut self, _context: &mut Context, frame: &mut ratatui::Frame) {
        let rect = center_subrect(frame.size(), (7, 10));
        frame.render_widget(widgets::Clear, rect);
        frame.render_widget(
            widgets::Table::new(HELP_INFO.iter().copied().flat_map(|(section, keys)| {
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
            .widths(&[layout::Constraint::Length(16), layout::Constraint::Percentage(80)])
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
