use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct PluginCommand;

pub struct PluginState {
    content: String,
    scroll: usize,
}

impl PluginCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "plugin",
        description: "Manage plugins",
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

impl TuiCommand for PluginCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(PluginState {
            content: "No plugins installed.\n\nUse 'atom plugin install <name>' to add plugins."
                .to_string(),
            scroll: 0,
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_mut::<PluginState>().unwrap();
        let block = Block::default()
            .title(" Plugins (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let lines: Vec<Line> = state
            .content
            .lines()
            .map(|l| {
                Line::from(Span::styled(
                    format!("  {}", l),
                    Style::default().fg(Color::White),
                ))
            })
            .collect();

        let paragraph = Paragraph::new(lines)
            .block(block)
            .scroll((state.scroll as u16, 0));
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<PluginState>().unwrap();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                state.scroll = state.scroll.saturating_sub(1);
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.scroll += 1;
                CommandAction::Continue
            }
            KeyCode::PageUp => {
                state.scroll = state.scroll.saturating_sub(10);
                CommandAction::Continue
            }
            KeyCode::PageDown => {
                state.scroll += 10;
                CommandAction::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
