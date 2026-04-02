use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct ModelCommand;

pub struct ModelState {
    pub info: String,
}

impl ModelCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "model",
        description: "Change model for current session",
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

impl TuiCommand for ModelCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(ModelState {
            info: "Use --model flag at startup to select model".to_string(),
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<ModelState>().unwrap();
        let block = Block::default()
            .title(" Model (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(state.info.clone(), Style::default().fg(Color::White)),
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
