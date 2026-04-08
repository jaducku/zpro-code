use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct SandboxToggleCommand;

pub struct SandboxToggleState {
    pub enabled: bool,
}

impl SandboxToggleCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "sandbox-toggle",
        description: "Toggle sandbox mode",
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

impl TuiCommand for SandboxToggleCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(SandboxToggleState { enabled: false })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_mut::<SandboxToggleState>().unwrap();
        let block = Block::default()
            .title(" Sandbox Mode (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let status_text = if state.enabled { "ON" } else { "OFF" };
        let status_color = if state.enabled {
            Color::Green
        } else {
            Color::Red
        };

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Sandbox mode: ", Style::default().fg(Color::DarkGray)),
                Span::styled(status_text, Style::default().fg(status_color).bold()),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Space/Enter to toggle | q to close",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        match key.code {
            KeyCode::Enter | KeyCode::Char(' ') => {
                let state = state.downcast_mut::<SandboxToggleState>().unwrap();
                state.enabled = !state.enabled;
                CommandAction::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
