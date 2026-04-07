use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct BranchCommand;

pub struct BranchState {
    pub branches: Vec<String>,
    pub current: String,
    pub selected: usize,
}

impl BranchCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "branch",
        description: "Switch or create git branches",
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

impl TuiCommand for BranchCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        let output = std::process::Command::new("git")
            .args(["branch", "-a"])
            .current_dir(&ctx.cwd)
            .output();

        let mut current = String::new();
        let mut branches = Vec::new();

        match output {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out.stdout);
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    if let Some(name) = trimmed.strip_prefix("* ") {
                        current = name.to_string();
                        branches.push(name.to_string());
                    } else {
                        branches.push(trimmed.to_string());
                    }
                }
            }
            Err(e) => {
                branches.push(format!("Error running git branch: {e}"));
            }
        }

        if branches.is_empty() {
            branches.push("(no branches found)".to_string());
        }

        Box::new(BranchState {
            branches,
            current,
            selected: 0,
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<BranchState>().unwrap();
        let block = Block::default()
            .title(" Git Branches (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let visible = (area.height as usize).saturating_sub(3);
        let scroll_offset = if state.selected >= visible {
            state.selected - visible + 1
        } else {
            0
        };
        let end = state.branches.len().min(scroll_offset + visible);

        let display_lines: Vec<Line> = state.branches[scroll_offset..end]
            .iter()
            .enumerate()
            .map(|(i, branch)| {
                let idx = scroll_offset + i;
                let is_current = *branch == state.current;
                let is_selected = idx == state.selected;

                let prefix = if is_current { "* " } else { "  " };
                let text = format!("  {prefix}{branch}");

                let mut style = if is_current {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::White)
                };

                if is_selected {
                    style = style.bg(Color::DarkGray);
                }

                Line::from(Span::styled(text, style))
            })
            .collect();

        let paragraph = Paragraph::new(display_lines).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<BranchState>().unwrap();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected > 0 {
                    state.selected -= 1;
                }
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected + 1 < state.branches.len() {
                    state.selected += 1;
                }
                CommandAction::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
