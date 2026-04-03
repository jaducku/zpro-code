use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;
use ratatui::widgets::*;

use crate::commands::context::CommandContext;
use crate::commands::traits::TuiCommand;
use crate::commands::types::{CommandAction, CommandMeta, CommandSource};

pub struct DoctorCommand;

pub struct DoctorState {
    pub checks: Vec<(String, bool)>,
}

impl DoctorCommand {
    pub(crate) const META: CommandMeta = CommandMeta {
        name: "doctor",
        description: "Run diagnostics",
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

fn check_command(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

impl TuiCommand for DoctorCommand {
    fn meta(&self) -> &CommandMeta {
        &Self::META
    }

    fn create_state(&self, ctx: &CommandContext) -> Box<dyn Any + Send> {
        let rust_ok = check_command("rustc");
        let git_ok = check_command("git");
        let cwd_ok = ctx.cwd.exists();
        let config_dir = crate::utils::paths::global_config_dir();
        let config_ok = config_dir.exists();

        let cwd_label = format!("Working directory ({})", ctx.cwd.display());

        let checks = vec![
            ("Rust toolchain".to_string(), rust_ok),
            ("Git available".to_string(), git_ok),
            (cwd_label, cwd_ok),
            ("Config dir (~/.atom)".to_string(), config_ok),
        ];

        Box::new(DoctorState { checks })
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &mut dyn Any) {
        let state = state.downcast_ref::<DoctorState>().unwrap();
        let block = Block::default()
            .title(" Diagnostics (q to close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let mut lines = vec![Line::from("")];

        for (name, ok) in &state.checks {
            let (icon, color) = if *ok {
                ("  \u{2714} ", Color::Green)
            } else {
                ("  \u{2718} ", Color::Red)
            };
            lines.push(Line::from(vec![
                Span::styled(icon, Style::default().fg(color).bold()),
                Span::styled(name.clone(), Style::default().fg(Color::White)),
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
