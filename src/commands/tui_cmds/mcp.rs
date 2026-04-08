use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct McpCommand;

pub struct McpState {
    servers: Vec<(String, String)>, // (name, command summary)
    selected: usize,
}

impl McpCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "mcp",
        description: "Manage MCP servers",
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

impl TuiCommand for McpCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        let configs = crate::services::mcp::config::read_mcp_json(&ctx.cwd);
        let mut servers: Vec<(String, String)> = configs
            .iter()
            .map(|(name, cfg)| {
                let summary = format!("{} {}", cfg.command, cfg.args.join(" "));
                (name.clone(), summary)
            })
            .collect();
        servers.sort_by(|a, b| a.0.cmp(&b.0));
        Box::new(McpState {
            servers,
            selected: 0,
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_mut::<McpState>().unwrap();
        let block = Block::default()
            .title(" MCP Servers (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        if state.servers.is_empty() {
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No MCP servers configured.",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Add with: atom mcp add-json <name> '<json>'",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            let paragraph = Paragraph::new(lines).block(block);
            frame.render_widget(Clear, area);
            frame.render_widget(paragraph, area);
        } else {
            let items: Vec<ListItem> = state
                .servers
                .iter()
                .enumerate()
                .map(|(i, (name, cmd))| {
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
                    let cmd_style = if is_selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {:<20}", name), name_style),
                        Span::styled(cmd.clone(), cmd_style),
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
        let state = state.downcast_mut::<McpState>().unwrap();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            KeyCode::Up | KeyCode::Char('k') => {
                state.selected = state.selected.saturating_sub(1);
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !state.servers.is_empty() {
                    state.selected = (state.selected + 1).min(state.servers.len() - 1);
                }
                CommandAction::Continue
            }
            _ => CommandAction::Continue,
        }
    }
}
