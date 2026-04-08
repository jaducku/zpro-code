use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct UltrareviewCommand;

pub struct UltrareviewState {}

impl UltrareviewCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "ultrareview",
        description: "Ultra-detailed code review",
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

impl TuiCommand for UltrareviewCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(UltrareviewState {})
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let _state = state.downcast_ref::<UltrareviewState>().unwrap();
        let block = Block::default()
            .title(" Ultra Review (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Ultra-detailed code review.",
                Style::default().fg(Color::White),
            )),
            Line::from(Span::styled(
                "  Runs an exhaustive security, performance, and correctness analysis.",
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  (Requires LLM connection)",
                Style::default().fg(Color::DarkGray),
            )),
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
