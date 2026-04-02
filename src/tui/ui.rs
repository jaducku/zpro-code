use ratatui::prelude::*;
use ratatui::widgets::*;

use super::app::{App, DisplayRole};

// Logo is rendered with colors in render_welcome(), not as a static string

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(frame: &mut Frame, app: &App) {
    let layout = Layout::vertical([
        Constraint::Min(1),    // Messages area
        Constraint::Length(3), // Input
        Constraint::Length(1), // Status bar
    ]);
    let chunks = layout.split(frame.area());

    render_messages(frame, chunks[0], app);
    render_input(frame, chunks[1], app);
    render_status_bar(frame, chunks[2], app);

    // Permission dialog overlay
    if let Some(dialog) = &app.permission_dialog {
        render_permission_dialog(frame, dialog);
    }
}

fn render_messages(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Show logo if no conversation yet (only system messages)
    let has_conversation = app.messages.iter().any(|m| {
        m.role == DisplayRole::User || m.role == DisplayRole::Assistant
    });

    if !has_conversation {
        render_welcome(&mut lines, app, area.width);
    } else {
        render_conversation(&mut lines, app, area.width);
    }

    // Auto-scroll
    let visible_height = area.height as usize;
    let skip = lines.len().saturating_sub(visible_height);

    let paragraph = Paragraph::new(lines).scroll((skip as u16, 0));
    frame.render_widget(paragraph, area);
}

fn render_welcome(lines: &mut Vec<Line<'static>>, app: &App, width: u16) {
    let w = width as usize;

    // Animated color based on elapsed time
    let elapsed = app.start_time.elapsed().as_millis() as usize;
    let orbit_colors = [Color::Cyan, Color::Blue, Color::Magenta, Color::LightCyan];
    let color_idx = (elapsed / 500) % orbit_colors.len();
    let nucleus_color = Color::Yellow;

    // 3 orbital colors cycle with animation
    let o1 = orbit_colors[color_idx % orbit_colors.len()];
    let o2 = orbit_colors[(color_idx + 1) % orbit_colors.len()];
    let o3 = orbit_colors[(color_idx + 2) % orbit_colors.len()];
    let electron = Color::LightYellow;

    // Electron position cycles (16 positions around orbits)
    let e_pos = (elapsed / 150) % 16;

    let center = |s: &str| -> String {
        let pad = w.saturating_sub(s.len()) / 2;
        format!("{:>pad$}{s}", "", pad = pad)
    };

    // Electron character at position
    let e = |pos: usize| -> &'static str {
        if e_pos == pos { "◉" } else { " " }
    };

    // Large Bohr atom — 3 intersecting orbits, wider and taller
    let art_w = 56;
    let p = w.saturating_sub(art_w) / 2;

    // Row 1
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled("               .·", Style::default().fg(o1)),
        Span::styled(e(0), Style::default().fg(electron).bold()),
        Span::styled("─────────────", Style::default().fg(o1)),
        Span::styled(e(8), Style::default().fg(electron).bold()),
        Span::styled("·.               ", Style::default().fg(o1)),
    ]));
    // Row 2
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled("           ·´", Style::default().fg(o1)),
        Span::styled("     /", Style::default().fg(o2)),
        Span::styled("               ", Style::default().fg(Color::Reset)),
        Span::styled("\\", Style::default().fg(o3)),
        Span::styled("     `·", Style::default().fg(o1)),
        Span::styled(format!("  {}", e(9)), Style::default().fg(electron).bold()),
    ]));
    // Row 3
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled("        ·´", Style::default().fg(o1)),
        Span::styled("     ·´", Style::default().fg(o2)),
        Span::styled("                   ", Style::default().fg(Color::Reset)),
        Span::styled("`·", Style::default().fg(o3)),
        Span::styled("     `·", Style::default().fg(o1)),
    ]));
    // Row 4
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled(format!("      /  {}  /", e(15)), Style::default().fg(o2)),
        Span::styled("                         ", Style::default().fg(Color::Reset)),
        Span::styled("\\", Style::default().fg(o3)),
        Span::styled("     \\", Style::default().fg(o2)),
    ]));
    // Row 5
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled("    |", Style::default().fg(o1)),
        Span::styled("   /", Style::default().fg(o2)),
        Span::styled("       ", Style::default().fg(Color::Reset)),
        Span::styled(format!(" {}  ", e(14)), Style::default().fg(electron).bold()),
        Span::styled("·", Style::default().fg(o3)),
        Span::styled("              ", Style::default().fg(Color::Reset)),
        Span::styled("·", Style::default().fg(o3)),
        Span::styled("  \\", Style::default().fg(o2)),
        Span::styled("   |", Style::default().fg(o1)),
    ]));
    // Row 6 — nucleus
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled("    |", Style::default().fg(o1)),
        Span::styled("  |", Style::default().fg(o2)),
        Span::styled("          ", Style::default().fg(Color::Reset)),
        Span::styled(" ⚛ ", Style::default().fg(nucleus_color).bold()),
        Span::styled("           ", Style::default().fg(Color::Reset)),
        Span::styled("|", Style::default().fg(o2)),
        Span::styled("  |", Style::default().fg(o1)),
    ]));
    // Row 7
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled("    |", Style::default().fg(o1)),
        Span::styled("   \\", Style::default().fg(o2)),
        Span::styled("            ", Style::default().fg(Color::Reset)),
        Span::styled("·", Style::default().fg(o3)),
        Span::styled("              ", Style::default().fg(Color::Reset)),
        Span::styled("·", Style::default().fg(o3)),
        Span::styled("  /", Style::default().fg(o2)),
        Span::styled("   |", Style::default().fg(o1)),
    ]));
    // Row 8
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled("      \\", Style::default().fg(o2)),
        Span::styled("     \\", Style::default().fg(o3)),
        Span::styled("                         ", Style::default().fg(Color::Reset)),
        Span::styled("/", Style::default().fg(o3)),
        Span::styled(format!("  {}  /", e(11)), Style::default().fg(electron).bold()),
    ]));
    // Row 9
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled("        `·", Style::default().fg(o1)),
        Span::styled("     `·", Style::default().fg(o3)),
        Span::styled("                   ", Style::default().fg(Color::Reset)),
        Span::styled("·´", Style::default().fg(o3)),
        Span::styled("     ·´", Style::default().fg(o1)),
    ]));
    // Row 10
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled(format!("     {}     `·", e(5)), Style::default().fg(electron).bold()),
        Span::styled("     \\", Style::default().fg(o3)),
        Span::styled("               ", Style::default().fg(Color::Reset)),
        Span::styled("/", Style::default().fg(o2)),
        Span::styled("     ·´", Style::default().fg(o1)),
    ]));
    // Row 11
    lines.push(Line::from(vec![
        Span::raw(format!("{:>p$}", "", p = p)),
        Span::styled("               `·", Style::default().fg(o1)),
        Span::styled(e(4), Style::default().fg(electron).bold()),
        Span::styled("─────────────", Style::default().fg(o1)),
        Span::styled(e(12), Style::default().fg(electron).bold()),
        Span::styled("·´               ", Style::default().fg(o1)),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // "A T O M" — simple, spaced, clean
    let atom_text = "A  T  O  M";
    lines.push(Line::from(Span::styled(
        center(atom_text),
        Style::default().fg(Color::Cyan).bold(),
    )));

    // Version
    let ver = format!("v{}", env!("CARGO_PKG_VERSION"));
    lines.push(Line::from(Span::styled(
        center(&ver),
        Style::default().fg(Color::DarkGray),
    )));

    lines.push(Line::from(""));

    // Connection status
    if app.has_provider {
        let msg = format!("● {}", app.model);
        lines.push(Line::from(Span::styled(
            center(&msg),
            Style::default().fg(Color::Green),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            center("○ no provider"),
            Style::default().fg(Color::DarkGray),
        )));
    }

    lines.push(Line::from(""));

    let hints_str = "Enter ↵ send  ·  Ctrl+C quit";
    lines.push(Line::from(Span::styled(
        center(hints_str),
        Style::default().fg(Color::DarkGray),
    )));
}

fn render_conversation(lines: &mut Vec<Line<'static>>, app: &App, _width: u16) {
    for msg in &app.messages {
        match msg.role {
            DisplayRole::System => {}
            DisplayRole::User => {
                lines.push(Line::from(""));
                let first = msg.content.lines().next().unwrap_or("").to_string();
                lines.push(Line::from(vec![
                    Span::styled(" > ", Style::default().fg(Color::Green).bold()),
                    Span::styled(first, Style::default().fg(Color::White).bold()),
                ]));
                for line in msg.content.lines().skip(1) {
                    lines.push(Line::from(format!("   {}", line)));
                }
            }
            DisplayRole::Assistant => {
                lines.push(Line::from(""));
                for line in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(Color::White),
                    )));
                }
            }
            DisplayRole::Tool => {
                let name = msg.tool_name.as_deref().unwrap_or("Tool").to_string();
                let content = truncate_content(&msg.content, 80);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {} ", name),
                        Style::default().fg(Color::Black).bg(Color::Magenta),
                    ),
                    Span::raw(" "),
                    Span::styled(content, Style::default().fg(Color::DarkGray)),
                ]));
            }
        }
    }

    // Loading spinner
    if app.is_loading {
        let elapsed = app.start_time.elapsed().as_millis() as usize;
        let frame_idx = (elapsed / 80) % SPINNER_FRAMES.len();
        let spinner = SPINNER_FRAMES[frame_idx];
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {} Thinking...", spinner),
            Style::default().fg(Color::Cyan),
        )));
    }
}

fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    let border_style = if app.is_loading {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Cyan)
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(if app.is_loading {
            Span::styled(" ... ", Style::default().fg(Color::DarkGray))
        } else {
            Span::styled(" > ", Style::default().fg(Color::Cyan).bold())
        });

    let input_text = Paragraph::new(app.input.as_str())
        .style(Style::default().fg(Color::White))
        .block(input_block);

    frame.render_widget(input_text, area);

    if !app.is_loading {
        frame.set_cursor_position((area.x + app.cursor_pos as u16 + 1, area.y + 1));
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let left = if app.has_provider {
        format!(" {} ", app.model)
    } else {
        " atom ".into()
    };

    let right = if app.token_count > 0 {
        format!("tokens: {} | ${:.4} ", app.token_count, app.session_cost)
    } else {
        String::new()
    };

    let middle_width = area.width as usize - left.len() - right.len();
    let middle = " ".repeat(middle_width.max(0));

    let status_line = Line::from(vec![
        Span::styled(&left, Style::default().fg(Color::Cyan).bold().bg(Color::DarkGray)),
        Span::styled(middle, Style::default().bg(Color::DarkGray)),
        Span::styled(&right, Style::default().fg(Color::DarkGray).bg(Color::DarkGray)),
    ]);

    frame.render_widget(Paragraph::new(status_line), area);
}

fn render_permission_dialog(frame: &mut Frame, dialog: &super::app::PermissionDialog) {
    let area = centered_rect(60, 30, frame.area());

    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(
            " Permission Required ",
            Style::default().fg(Color::Yellow).bold(),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Tool: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&dialog.tool_name, Style::default().fg(Color::White).bold()),
        ]),
        Line::from(""),
        Line::from(format!("  {}", dialog.description)),
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            if dialog.selected {
                Span::styled(" Allow ", Style::default().fg(Color::Black).bg(Color::Green).bold())
            } else {
                Span::styled(" Allow ", Style::default().fg(Color::DarkGray))
            },
            Span::raw("   "),
            if !dialog.selected {
                Span::styled(" Deny ", Style::default().fg(Color::Black).bg(Color::Red).bold())
            } else {
                Span::styled(" Deny ", Style::default().fg(Color::DarkGray))
            },
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn truncate_content(content: &str, max_width: usize) -> String {
    let first_line = content.lines().next().unwrap_or("");
    if first_line.len() > max_width {
        format!("{}...", &first_line[..max_width.saturating_sub(3)])
    } else if content.lines().count() > 1 {
        format!("{} (+{} lines)", first_line, content.lines().count() - 1)
    } else {
        first_line.to_string()
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let layout_v = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ]);
    let layout_h = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ]);
    let v_chunks = layout_v.split(area);
    let h_chunks = layout_h.split(v_chunks[1]);
    h_chunks[1]
}
