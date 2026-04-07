use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct ExtraUsageCommand;

pub struct ExtraUsageState {}

impl ExtraUsageCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "extra-usage",
        description: "Show extra usage information",
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

impl TuiCommand for ExtraUsageCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, _ctx: &CommandContext) -> Box<dyn Any + Send> {
        Box::new(ExtraUsageState {})
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let _state = state.downcast_ref::<ExtraUsageState>().unwrap();
        let block = Block::default()
            .title(" Extra Usage (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Additional Usage Tips",
                Style::default().fg(Color::White).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Keyboard:",
                Style::default().fg(Color::White).bold(),
            )),
            Line::from(vec![
                Span::styled("    Ctrl+C      ", Style::default().fg(Color::Cyan)),
                Span::styled("Quit", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("    /           ", Style::default().fg(Color::Cyan)),
                Span::styled("Command menu", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("    @           ", Style::default().fg(Color::Cyan)),
                Span::styled("File mention", Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  CLI Flags:",
                Style::default().fg(Color::White).bold(),
            )),
            Line::from(vec![
                Span::styled("    --model     ", Style::default().fg(Color::Cyan)),
                Span::styled("Set model", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("    --print     ", Style::default().fg(Color::Cyan)),
                Span::styled("Non-interactive mode", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("    --api-base  ", Style::default().fg(Color::Cyan)),
                Span::styled("Set provider URL", Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("    --help      ", Style::default().fg(Color::Cyan)),
                Span::styled("Show all options", Style::default().fg(Color::White)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  Press q to close",
                Style::default().fg(Color::DarkGray),
            )),
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
