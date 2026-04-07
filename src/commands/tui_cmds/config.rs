use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct ConfigCommand;

fn find_instructions_file(cwd: &std::path::Path) -> String {
    // Check for AGENTS.md (open standard)
    let agents_md = cwd.join("AGENTS.md");
    if agents_md.exists() {
        return agents_md.display().to_string();
    }

    // Check for .agents/AGENTS.md
    let dot_agents = cwd.join(".agents/AGENTS.md");
    if dot_agents.exists() {
        return dot_agents.display().to_string();
    }

    // Check for CLAUDE.md (legacy)
    let claude_md = cwd.join("CLAUDE.md");
    if claude_md.exists() {
        return format!("{} (legacy)", claude_md.display());
    }

    "(not set)".to_string()
}

pub struct ConfigState {
    pub items: Vec<(String, String)>,
    pub selected: usize,
}

impl ConfigCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "config",
        description: "Open config panel",
        aliases: &["settings"],
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

impl TuiCommand for ConfigCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        // Read actual permission mode from app state
        let permission_mode = {
            let state = ctx.app_state.try_read();
            state
                .map(|s| format!("{:?}", s.tool_permission_context.mode))
                .unwrap_or_else(|_| "default".to_string())
                .to_lowercase()
        };

        // Detect model from app state or environment
        let model =
            std::env::var("ATOM_MODEL").unwrap_or_else(|_| "(from --model flag)".to_string());

        // Find AGENTS.md (project instructions)
        let agents_md = find_instructions_file(&ctx.cwd);

        // Count MCP servers
        let mcp_configs = crate::services::mcp::config::read_mcp_json(&ctx.cwd);
        let mcp_count = mcp_configs.len();

        // Count loaded skills
        let skills = crate::services::skills::load_skills(&ctx.cwd);
        let skill_count = skills.len();

        let items = vec![
            ("Permission mode".to_string(), permission_mode),
            ("Model".to_string(), model),
            ("Theme".to_string(), ctx.options.theme.clone()),
            (
                "Working directory".to_string(),
                ctx.cwd.display().to_string(),
            ),
            ("Project instructions".to_string(), agents_md),
            (
                "MCP servers".to_string(),
                if mcp_count > 0 {
                    format!("{} configured", mcp_count)
                } else {
                    "none".to_string()
                },
            ),
            (
                "Skills".to_string(),
                if skill_count > 0 {
                    format!("{} loaded", skill_count)
                } else {
                    "none".to_string()
                },
            ),
            (
                "Version".to_string(),
                format!("v{}", env!("CARGO_PKG_VERSION")),
            ),
        ];

        Box::new(ConfigState { items, selected: 0 })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<ConfigState>().unwrap();
        let block = Block::default()
            .title(" Settings (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let mut lines = vec![Line::from("")];

        for (i, (name, value)) in state.items.iter().enumerate() {
            let marker = if i == state.selected { "> " } else { "  " };
            let style = if i == state.selected {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::White)
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{marker}{name}: "),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(value.clone(), style),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Up/Down to scroll | q to close",
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&self, key: KeyEvent, state: &mut dyn Any) -> CommandAction {
        let state = state.downcast_mut::<ConfigState>().unwrap();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected > 0 {
                    state.selected -= 1;
                }
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected + 1 < state.items.len() {
                    state.selected += 1;
                }
                CommandAction::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
