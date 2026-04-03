use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct SkillsCommand;

pub struct SkillsState {
    skills: Vec<(String, String)>, // (name, description)
    selected: usize,
}

impl SkillsCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "skills",
        description: "Manage skills",
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

impl TuiCommand for SkillsCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        let loaded = crate::services::skills::load_skills(&ctx.cwd);
        let mut skills: Vec<(String, String)> = loaded
            .iter()
            .map(|cmd| {
                let meta = cmd.meta();
                (meta.name.to_string(), meta.description.to_string())
            })
            .collect();
        skills.sort_by(|a, b| a.0.cmp(&b.0));
        Box::new(SkillsState {
            skills,
            selected: 0,
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_mut::<SkillsState>().unwrap();
        let block = Block::default()
            .title(" Skills (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        if state.skills.is_empty() {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No skills loaded.",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Add skills in .agents/skills/ or ~/.atom/skills/",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(Clear, area);
            frame.render_widget(paragraph, area);
        } else {
            let items: Vec<ListItem> = state
                .skills
                .iter()
                .enumerate()
                .map(|(i, (name, desc))| {
                    let is_selected = i == state.selected;
                    let style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan).bold()
                    } else {
                        Style::default()
                    };
                    let name_style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan).bold()
                    } else {
                        Style::default().fg(Color::Green)
                    };
                    let desc_style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {:<20}", name), name_style),
                        Span::styled(desc.clone(), desc_style),
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
        let state = state.downcast_mut::<SkillsState>().unwrap();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            KeyCode::Up | KeyCode::Char('k') => {
                state.selected = state.selected.saturating_sub(1);
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !state.skills.is_empty() {
                    state.selected = (state.selected + 1).min(state.skills.len() - 1);
                }
                CommandAction::Continue
            }
            _ => CommandAction::Continue,
        }
    }
}
