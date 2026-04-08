use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct PrivacySettingsCommand;

pub struct PrivacySettingsState {
    pub settings: Vec<(String, String)>,
}

impl PrivacySettingsCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "privacy-settings",
        description: "Configure privacy settings",
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

impl TuiCommand for PrivacySettingsCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(PrivacySettingsState {
            settings: vec![
                ("Analytics".to_string(), "enabled".to_string()),
                ("Session persistence".to_string(), "enabled".to_string()),
                ("Error reporting".to_string(), "enabled".to_string()),
            ],
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<PrivacySettingsState>().unwrap();
        let block = Block::default()
            .title(" Privacy Settings (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let mut lines = vec![Line::from("")];
        for (name, value) in &state.settings {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<24}", name),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(value.clone(), Style::default().fg(Color::White)),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Press q to close",
            Style::default().fg(Color::DarkGray),
        )));

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
