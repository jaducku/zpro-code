use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct FilesCommand;

pub struct FilesState {
    pub files: Vec<String>,
    pub selected: usize,
    pub scroll: usize,
}

impl FilesCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "files",
        description: "List tracked files",
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

impl TuiCommand for FilesCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        let mut files = Vec::new();
        for entry in ignore::WalkBuilder::new(&ctx.cwd)
            .max_depth(Some(1))
            .build()
            .flatten()
        {
            if entry.path() == ctx.cwd {
                continue;
            }
            let name = entry
                .path()
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !name.is_empty() {
                let display = if entry.path().is_dir() {
                    format!("{name}/")
                } else {
                    name
                };
                files.push(display);
            }
        }
        files.sort();

        Box::new(FilesState {
            files,
            selected: 0,
            scroll: 0,
        })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<FilesState>().unwrap();
        let block = Block::default()
            .title(format!(" Files ({}) (q to close) ", state.files.len()))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        // visible area height: total area minus borders (2) minus header/footer lines (3)
        let visible = (area.height as usize).saturating_sub(5);

        let mut lines = vec![Line::from("")];

        let end = state.files.len().min(state.scroll + visible);
        for i in state.scroll..end {
            let marker = if i == state.selected { "> " } else { "  " };
            let style = if i == state.selected {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(Span::styled(
                format!("{marker}{}", state.files[i]),
                style,
            )));
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
        let state = state.downcast_mut::<FilesState>().unwrap();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if state.selected > 0 {
                    state.selected -= 1;
                    if state.selected < state.scroll {
                        state.scroll = state.selected;
                    }
                }
                CommandAction::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.selected + 1 < state.files.len() {
                    state.selected += 1;
                    // scroll follows selection (keep a margin of ~0)
                    if state.selected >= state.scroll + 20 {
                        state.scroll = state.selected.saturating_sub(19);
                    }
                }
                CommandAction::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => CommandAction::Done(None),
            _ => CommandAction::Continue,
        }
    }
}
