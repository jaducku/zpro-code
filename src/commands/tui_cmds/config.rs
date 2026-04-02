use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct ConfigCommand;

pub struct ConfigState {
    pub items: Vec<(String, String)>,
    pub selected: usize,
}

impl ConfigCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "config",
        description: "Open config panel",
        aliases: &["settings"],
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

impl TuiCommand for ConfigCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let instructions_path = home.join(".claude").join("CLAUDE.md");
        let instructions_value = if instructions_path.exists() {
            instructions_path.display().to_string()
        } else {
            "(not set)".to_string()
        };

        let items = vec![
            ("Permission mode".to_string(), "default".to_string()),
            ("Model".to_string(), "claude-sonnet-4-20250514".to_string()),
            ("Theme".to_string(), ctx.options.theme.clone()),
            ("Vim mode".to_string(), "off".to_string()),
            ("Custom instructions".to_string(), instructions_value),
        ];

        Box::new(ConfigState { items, selected: 0 })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<ConfigState>().unwrap();
        let block = Block::default()
            .title(" Settings (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let mut lines = vec![Line::from("")];

        for (i, (name, value)) in state.items.iter().enumerate() {
            let marker = if i == state.selected { "> " } else { "  " };
            let style = if i == state.selected {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::White)
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{marker}{name}: "), Style::default().fg(Color::DarkGray)),
                Span::styled(value.clone(), style),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Up/Down to scroll | q to close",
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<ConfigState>().unwrap();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected > 0 {
                    state.selected -= 1;
                }
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected + 1 < state.items.len() {
                    state.selected += 1;
                }
                CommandAction::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
