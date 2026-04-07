use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct StatusCommand;

pub struct StatusState {
    pub version: String,
    pub cwd: String,
}

impl StatusCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "status",
        description: "Show Claude Code status including version, model, account, API connectivity, and tool statuses",
        aliases: &[],
        is_hidden: false,
        immediate: true,
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

impl TuiCommand for StatusCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(StatusState {
            version: format!("v{}", env!("CARGO_PKG_VERSION")),
            cwd: ctx.cwd.display().to_string(),
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<StatusState>().unwrap();
        let block = Block::default()
            .title(" Status (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Version:    ", Style::default().fg(Color::DarkGray)),
                Span::styled(state.version.clone(), Style::default().fg(Color::White).bold()),
            ]),
            Line::from(vec![
                Span::styled("  Directory:  ", Style::default().fg(Color::DarkGray)),
                Span::styled(state.cwd.clone(), Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(Span::styled("  Press q to close", Style::default().fg(Color::DarkGray))),
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
