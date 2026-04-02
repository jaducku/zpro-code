use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct ContextCommand;

pub struct ContextState {
    pub claude_md_exists: bool,
    pub claude_md_preview: Option<String>,
    pub cwd: String,
}

impl ContextCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "context",
        description: "Show context information",
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

impl TuiCommand for ContextCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        let claude_md = ctx.cwd.join("CLAUDE.md");
        let (exists, preview) = if claude_md.exists() {
            let content = std::fs::read_to_string(&claude_md).unwrap_or_default();
            let preview: String = content.lines().take(10).collect::<Vec<_>>().join("\n");
            (true, Some(preview))
        } else {
            (false, None)
        };

        Box::new(ContextState {
            claude_md_exists: exists,
            claude_md_preview: preview,
            cwd: ctx.cwd.display().to_string(),
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<ContextState>().unwrap();
        let block = Block::default()
            .title(" Context Info (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let mut lines: Vec<Line> = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Working directory: ", Style::default().fg(Color::White)),
                Span::styled(&state.cwd, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
        ];

        if state.claude_md_exists {
            lines.push(Line::from(vec![
                Span::styled("  CLAUDE.md: ", Style::default().fg(Color::White)),
                Span::styled("found", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Preview (first 10 lines):",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));

            if let Some(ref preview) = state.claude_md_preview {
                for line in preview.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("    {line}"),
                        Style::default().fg(Color::Gray),
                    )));
                }
            }
        } else {
            lines.push(Line::from(vec![
                Span::styled("  CLAUDE.md: ", Style::default().fg(Color::White)),
                Span::styled(
                    "not found",
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Create a CLAUDE.md file to provide project-specific instructions.",
                Style::default().fg(Color::Gray),
            )));
        }

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
