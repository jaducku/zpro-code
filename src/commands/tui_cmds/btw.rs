use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandDoneResult, CommandMeta, CommandSource};

pub struct BtwCommand;

pub struct BtwState {
    pub input: String,
    pub cursor: usize,
}

impl BtwCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "btw",
        description: "Send a quick note",
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

impl TuiCommand for BtwCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(BtwState {
            input: String::new(),
            cursor: 0,
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<BtwState>().unwrap();
        let block = Block::default()
            .title(" Quick Note (Enter to send, Esc to cancel) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let mut display = state.input.clone();
        display.insert(state.cursor, '|');

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  > {display}"),
                Style::default().fg(Color::White),
            )),
            Line::from(""),
        ];

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<BtwState>().unwrap();
        match key.code {
            KeyCode::Char(c) => {
                state.input.insert(state.cursor, c);
                state.cursor += 1;
                CommandAction::Continue
            }
            KeyCode::Backspace => {
                if state.cursor > 0 {
                    state.cursor -= 1;
                    state.input.remove(state.cursor);
                }
                CommandAction::Continue
            }
            KeyCode::Left => {
                if state.cursor > 0 {
                    state.cursor -= 1;
                }
                CommandAction::Continue
            }
            KeyCode::Right => {
                if state.cursor < state.input.len() {
                    state.cursor += 1;
                }
                CommandAction::Continue
            }
            KeyCode::Enter => {
                let text = state.input.clone();
                CommandAction::Done(Some(CommandDoneResult {
                    text: Some(text),
                    ..Default::default()
                }))
            }
            KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
