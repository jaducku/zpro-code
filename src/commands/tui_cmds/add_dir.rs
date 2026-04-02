use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct AddDirCommand;

pub struct AddDirState;

impl AddDirCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "add-dir",
        description: "Add directory to allowed paths",
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

impl TuiCommand for AddDirCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(AddDirState)
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let _state = state.downcast_mut::<AddDirState>().unwrap();
        let block = Block::default()
            .title(format!(" {} (q to close) ", Self::META.description))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));
        let text = Paragraph::new("Add directory to allowed paths")
            .block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(text, area);
    }

    fn handle_key(&self, key: KeyEvent, _state: &mut dyn Any) -> CommandAction {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
