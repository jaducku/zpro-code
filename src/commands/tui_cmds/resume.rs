use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandDoneResult, CommandMeta, CommandSource};
use crate::services::session_transcript::SessionStore;

pub struct ResumeCommand;

pub struct ResumeState {
    sessions: Vec<(String, String)>, // (session_id, info_line)
    selected: usize,
}

impl ResumeCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "resume",
        description: "Resume a previous session",
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

impl TuiCommand for ResumeCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        let sessions = SessionStore::new()
            .and_then(|s| s.list())
            .unwrap_or_default();
        let items: Vec<(String, String)> = sessions
            .iter()
            .map(|s| {
                (
                    s.session_id.clone(),
                    format!(
                        "{} | {} msgs | {}",
                        s.model,
                        s.message_count,
                        s.created_at.format("%m-%d %H:%M")
                    ),
                )
            })
            .collect();
        Box::new(ResumeState {
            sessions: items,
            selected: 0,
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_mut::<ResumeState>().unwrap();
        let block = Block::default()
            .title(" Resume Session (Enter to select, q to cancel) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        if state.sessions.is_empty() {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No sessions to resume.",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(Clear, area);
            frame.render_widget(paragraph, area);
        } else {
            let items: Vec<ListItem> = state
                .sessions
                .iter()
                .enumerate()
                .map(|(i, (id, info))| {
                    let is_selected = i == state.selected;
                    let style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan).bold()
                    } else {
                        Style::default()
                    };
                    let id_style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan).bold()
                    } else {
                        Style::default().fg(Color::Green)
                    };
                    let info_style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {:<20}", id), id_style),
                        Span::styled(info.clone(), info_style),
                    ]))
                    .style(style)
                })
                .collect();

            let list = List::new(items).block(block);
            frame.render_widget(Clear, area);
            frame.render_widget(list, area);
        }
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<ResumeState>().unwrap();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            KeyCode::Enter => {
                if let Some((session_id, _)) = state.sessions.get(state.selected) {
                    CommandAction::Done(Some(CommandDoneResult {
                        text: Some(session_id.clone()),
                        ..Default::default()
                    }))
                } else {
                    CommandAction::Done(None)
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.selected = state.selected.saturating_sub(1);
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !state.sessions.is_empty() {
                    state.selected = (state.selected + 1).min(state.sessions.len() - 1);
                }
                CommandAction::Continue
            }
            _ => CommandAction::Continue,
        }
    }
}
