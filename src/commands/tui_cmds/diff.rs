use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct DiffCommand;

pub struct DiffState {
    pub lines: Vec<String>,
    pub scroll: usize,
}

impl DiffCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "diff",
        description: "Show diff of changes",
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

impl TuiCommand for DiffCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        let output = std::process::Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(&ctx.cwd)
            .output();

        let lines = match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                if text.is_empty() {
                    vec!["(no changes)".to_string()]
                } else {
                    text.lines().map(|l| l.to_string()).collect()
                }
            }
            Err(e) => vec![format!("Error running git diff: {e}")],
        };

        Box::new(DiffState { lines, scroll: 0 })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<DiffState>().unwrap();
        let block = Block::default()
            .title(" Git Diff (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let visible = (area.height as usize).saturating_sub(3);
        let end = state.lines.len().min(state.scroll + visible);

        let display_lines: Vec<Line> = state.lines[state.scroll..end]
            .iter()
            .map(|line| {
                let style = if line.starts_with('+') {
                    Style::default().fg(Color::Green)
                } else if line.starts_with('-') {
                    Style::default().fg(Color::Red)
                } else if line.starts_with("@@") {
                    Style::default().fg(Color::Cyan)
                } else if line.starts_with("diff ") || line.starts_with("index ") {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(format!("  {line}"), style))
            })
            .collect();

        let paragraph = Paragraph::new(display_lines).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<DiffState>().unwrap();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if state.scroll > 0 {
                    state.scroll -= 1;
                }
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.scroll + 1 < state.lines.len() {
                    state.scroll += 1;
                }
                CommandAction::Continue
            }
            KeyCode::PageUp => {
                state.scroll = state.scroll.saturating_sub(20);
                CommandAction::Continue
            }
            KeyCode::PageDown => {
                state.scroll = (state.scroll + 20).min(state.lines.len().saturating_sub(1));
                CommandAction::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
