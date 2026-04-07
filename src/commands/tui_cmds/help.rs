use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta};

pub struct HelpCommand;

pub struct HelpState {
    pub scroll: u16,
    pub commands: Vec<(String, String)>,
}

impl HelpCommand {
    const META: CommandMeta = CommandMeta {
        name: "help",
        description: "Show help and available commands",
        aliases: &["?"],
        is_hidden: false,
        immediate: false,
        argument_hint: None,
        availability: &[],
        source: crate::commands::types::CommandSource::Builtin,
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

impl TuiCommand for HelpCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(HelpState {
            scroll: 0,
            commands: vec![
                ("clear".into(), "Clear conversation history and free up context".into()),
                ("review".into(), "Review a pull request".into()),
                ("help".into(), "Show help and available commands".into()),
                ("compact".into(), "Compact conversation to save context".into()),
                ("cost".into(), "Show token usage and session cost".into()),
                ("model".into(), "Show or change the current model".into()),
                ("vim".into(), "Toggle vim keybindings".into()),
                ("theme".into(), "Change the color theme".into()),
            ],
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_mut::<HelpState>().expect("HelpState");

        let items: Vec<ListItem> = state
            .commands
            .iter()
            .map(|(name, desc)| {
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  /{:<12}", name),
                        Style::default().fg(Color::Green).bold(),
                    ),
                    Span::styled(desc.clone(), Style::default().fg(Color::White)),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(Span::styled(
                        " Help — Available Commands ",
                        Style::default().fg(Color::Cyan).bold(),
                    ))
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .highlight_style(Style::default().bg(Color::DarkGray));

        let max_scroll = (state.commands.len() as u16).saturating_sub(area.height.saturating_sub(2));
        if state.scroll > max_scroll {
            state.scroll = max_scroll;
        }

        // Render footer hint
        let inner = area.inner(Margin::new(1, 1));
        let layout = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
        ]);
        let chunks = layout.split(inner);

        // Render the list in the main area
        frame.render_widget(
            list.clone(),
            area,
        );

        // Render footer hint over the bottom of the block
        let hint = Line::from(vec![
            Span::styled(
                " q/Esc: close  |  j/k/Up/Down: scroll ",
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        if chunks[1].height > 0 {
            frame.render_widget(Paragraph::new(hint), chunks[1]);
        }
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<HelpState>().expect("HelpState");

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            KeyCode::Up | KeyCode::Char('k') => {
                state.scroll = state.scroll.saturating_sub(1);
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.scroll = state.scroll.saturating_add(1);
                CommandAction::Continue
            }
            _ => CommandAction::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_meta() {
        let cmd = HelpCommand;
        let meta = cmd.meta();
        assert_eq!(meta.name, "help");
        assert_eq!(meta.aliases, &["?"]);
    }

    #[test]
    fn test_help_handle_key_quit() {
        let cmd = HelpCommand;
        let mut state: Box<dyn Any + Send> = Box::new(HelpState {
            scroll: 0,
            commands: vec![],
        });
        let action = cmd.handle_key(KeyEvent::from(KeyCode::Char('q')), state.as_mut());
        assert!(matches!(action, CommandAction::Done(_)));
    }

    #[test]
    fn test_help_handle_key_esc() {
        let cmd = HelpCommand;
        let mut state: Box<dyn Any + Send> = Box::new(HelpState {
            scroll: 0,
            commands: vec![],
        });
        let action = cmd.handle_key(KeyEvent::from(KeyCode::Esc), state.as_mut());
        assert!(matches!(action, CommandAction::Done(_)));
    }

    #[test]
    fn test_help_handle_key_scroll() {
        let cmd = HelpCommand;
        let mut state: Box<dyn Any + Send> = Box::new(HelpState {
            scroll: 0,
            commands: vec![("test".into(), "desc".into())],
        });

        cmd.handle_key(KeyEvent::from(KeyCode::Down), state.as_mut());
        let s = state.downcast_ref::<HelpState>().unwrap();
        assert_eq!(s.scroll, 1);

        cmd.handle_key(KeyEvent::from(KeyCode::Up), state.as_mut());
        let s = state.downcast_ref::<HelpState>().unwrap();
        assert_eq!(s.scroll, 0);

        // Can't scroll below 0
        cmd.handle_key(KeyEvent::from(KeyCode::Up), state.as_mut());
        let s = state.downcast_ref::<HelpState>().unwrap();
        assert_eq!(s.scroll, 0);
    }
}
