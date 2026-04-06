use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandDoneResult, CommandMeta, CommandSource};

pub struct ModelCommand;

pub struct ModelState {
    /// (alias, full_model_id, is_current)
    pub models: Vec<(String, String, bool)>,
    pub selected: usize,
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
        let config = crate::services::config::AtomConfig::load();
        let current = std::env::var("ATOM_CURRENT_MODEL").unwrap_or_default();
        let models: Vec<(String, String, bool)> = config
            .list_models()
            .into_iter()
            .map(|(alias, full_id)| {
                let is_current = full_id == current || alias == current;
                (alias, full_id, is_current)
            })
            .collect();

        Box::new(ModelState {
            selected: 0,
            models,
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<ModelState>().unwrap();
        let block = Block::default()
            .title(" Model (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        if state.models.is_empty() {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No models configured.",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Create ~/.zpro-code/config.json with:",
                    Style::default().fg(Color::White),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  {",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "    \"provider\": \"openrouter\",",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "    \"api_key\": \"sk-...\",",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "    \"default_model\": \"fast\",",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "    \"models\": {",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "      \"fast\": \"google/gemma-3-27b-it:free\",",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "      \"smart\": \"anthropic/claude-sonnet-4\"",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "    }",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(Span::styled(
                    "  }",
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
            return;
        }

        let mut lines = vec![Line::from("")];

        for (i, (alias, full_id, is_current)) in state.models.iter().enumerate() {
            let marker = if *is_current { "* " } else { "  " };
            let cursor = if i == state.selected { "> " } else { "  " };

            let alias_style = if i == state.selected {
                Style::default().fg(Color::Green).bold()
            } else {
                Style::default().fg(Color::Green)
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{cursor}{marker}"), Style::default().fg(Color::Cyan)),
                Span::styled(format!("{alias:<12}"), alias_style),
                Span::styled(format!(" {full_id}"), Style::default().fg(Color::DarkGray)),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Enter to select, q to close",
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<ModelState>().unwrap();

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            KeyCode::Up | KeyCode::Char('k') => {
                if !state.models.is_empty() && state.selected > 0 {
                    state.selected -= 1;
                }
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !state.models.is_empty() && state.selected < state.models.len() - 1 {
                    state.selected += 1;
                }
                CommandAction::Continue
            }
            KeyCode::Enter => {
                if let Some((alias, _full_id, _)) = state.models.get(state.selected) {
                    CommandAction::Done(Some(CommandDoneResult {
                        text: Some(alias.clone()), // alias for registry resolve
                        ..Default::default()
                    }))
                } else {
                    CommandAction::Done(None)
                }
            }
            _ => CommandAction::Continue,
        }
    }
}
