use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct UsageCommand;

pub struct UsageState {
    pub version: String,
    pub cwd: String,
}

impl UsageCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "usage",
        description: "Show usage information",
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

impl TuiCommand for UsageCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(UsageState {
            version: env!("CARGO_PKG_VERSION").to_string(),
            cwd: ctx.cwd.display().to_string(),
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<UsageState>().unwrap();
        let block = Block::default()
            .title(" Usage Info (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let label_style = Style::default().fg(Color::White);
        let value_style = Style::default().fg(Color::Yellow);
        let heading_style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);
        let cmd_style = Style::default().fg(Color::Cyan);
        let desc_style = Style::default().fg(Color::Gray);

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Version:           ", label_style),
                Span::styled(&state.version, value_style),
            ]),
            Line::from(vec![
                Span::styled("  Working directory: ", label_style),
                Span::styled(&state.cwd, value_style),
            ]),
            Line::from(""),
            Line::from(Span::styled("  Useful Commands:", heading_style)),
            Line::from(""),
            Line::from(vec![
                Span::styled("  /help        ", cmd_style),
                Span::styled("Show help and available commands", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  /config      ", cmd_style),
                Span::styled("Open configuration", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  /status      ", cmd_style),
                Span::styled("Show current status", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  /diff        ", cmd_style),
                Span::styled("Show git diff of changes", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  /branch      ", cmd_style),
                Span::styled("List git branches", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  /context     ", cmd_style),
                Span::styled("Show context information", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  /permissions ", cmd_style),
                Span::styled("View permission settings", desc_style),
            ]),
            Line::from(vec![
                Span::styled("  /hooks       ", cmd_style),
                Span::styled("Manage hooks configuration", desc_style),
            ]),
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
