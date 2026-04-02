use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandDoneResult, CommandMeta, CommandSource};

pub struct OutputStyleCommand;

pub struct OutputStyleState {
    pub styles: Vec<String>,
    pub selected: usize,
}

impl OutputStyleCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "output-style",
        description: "Change output style",
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

impl TuiCommand for OutputStyleCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        let styles = vec![
            "default".to_string(),
            "concise".to_string(),
            "verbose".to_string(),
            "json".to_string(),
        ];
        Box::new(OutputStyleState { styles, selected: 0 })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<OutputStyleState>().unwrap();
        let block = Block::default()
            .title(" Output Style (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let mut lines = vec![Line::from("")];

        for (i, style) in state.styles.iter().enumerate() {
            let marker = if i == state.selected { "> " } else { "  " };
            let text_style = if i == state.selected {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(format!("{marker}{style}"), text_style)));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Up/Down to select | Enter to apply | q to close",
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<OutputStyleState>().unwrap();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected > 0 {
                    state.selected -= 1;
                }
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected + 1 < state.styles.len() {
                    state.selected += 1;
                }
                CommandAction::Continue
            }
            KeyCode::Enter => {
                let chosen = state.styles[state.selected].clone();
                CommandAction::Done(Some(CommandDoneResult {
                    text: Some(chosen),
                    ..Default::default()
                }))
            }
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
