use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct TerminalSetupCommand;

pub struct TerminalSetupState {
    pub term: String,
    pub shell: String,
    pub cols: u16,
    pub rows: u16,
}

impl TerminalSetupCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "terminal-setup",
        description: "Configure terminal",
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

impl TuiCommand for TerminalSetupCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        let term = std::env::var("TERM").unwrap_or_else(|_| "unknown".into());
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "unknown".into());
        let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
        Box::new(TerminalSetupState {
            term,
            shell,
            cols,
            rows,
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<TerminalSetupState>().unwrap();
        let block = Block::default()
            .title(" Terminal Setup (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Terminal Info",
                Style::default().fg(Color::White).bold(),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("  TERM:       ", Style::default().fg(Color::DarkGray)),
                Span::styled(state.term.clone(), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  Shell:      ", Style::default().fg(Color::DarkGray)),
                Span::styled(state.shell.clone(), Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  Size:       ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}x{}", state.cols, state.rows),
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Press q to close",
                Style::default().fg(Color::DarkGray),
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
