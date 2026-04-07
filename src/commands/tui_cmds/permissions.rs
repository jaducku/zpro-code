use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct PermissionsCommand;

pub struct PermissionsState {
    pub mode: String,
}

impl PermissionsCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "permissions",
        description: "View and manage permissions",
        aliases: &[],
        is_hidden: false,
        immediate: false,
        argument_hint: None,
        availability: &[],
        source: CommandSource::Builtin,
        loaded_from: None,
        when_to_use: None,
        is_sensitive: false,
        is_enabled: true,
        user_invocable: None,
        disable_model_invocation: false,
        kind: None,
        version: None,
        has_user_specified_description: false,
        is_mcp: false,
        plugin_info: None,
        user_facing_name: None,
    };
}

impl TuiCommand for PermissionsCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(PermissionsState {
            mode: "default".to_string(),
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<PermissionsState>().unwrap();
        let block = Block::default()
            .title(" Permissions (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let mode_style = match state.mode.as_str() {
            "auto" => Style::default().fg(Color::Yellow),
            "bypass" => Style::default().fg(Color::Red),
            _ => Style::default().fg(Color::Green),
        };

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Current mode: ", Style::default().fg(Color::White)),
                Span::styled(&state.mode, mode_style.add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Permission Modes:",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "  default  ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "- Ask before running tools that could modify files or",
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(Span::styled(
                "             execute commands. Recommended for most use.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "  auto     ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "- Automatically approve tool calls that match your",
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(Span::styled(
                "             allowlist. Prompts for everything else.",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "  bypass   ",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    "- Skip all permission prompts. Use with caution.",
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(Span::styled(
                "             Not recommended for untrusted projects.",
                Style::default().fg(Color::Gray),
            )),
        ];

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, _state: &mut dyn Any) -> CommandAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
