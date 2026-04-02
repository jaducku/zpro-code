use ratatui::prelude::*;
use ratatui::widgets::*;

use super::app::{App, DisplayRole};

pub fn render(frame: &mut Frame, app: &App) {
    let layout = Layout::vertical([
        Constraint::Min(1),    // Messages
        Constraint::Length(1), // Separator
        Constraint::Length(3), // Input
        Constraint::Length(1), // Status bar
    ]);
    let chunks = layout.split(frame.area());

    render_messages(frame, chunks[0], app);
    render_separator(frame, chunks[1]);
    render_input(frame, chunks[2], app);
    render_status_bar(frame, chunks[3], app);

    // Permission dialog overlay
    if let Some(dialog) = &app.permission_dialog {
        render_permission_dialog(frame, dialog);
    }
}

fn render_messages(frame: &mut Frame, area: Rect, app: &App) {
    let max_w = area.width as usize;

    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        match msg.role {
            DisplayRole::System => {
                lines.push(Line::from(Span::styled(
                    format!("  {}", msg.content),
                    Style::default().fg(Color::DarkGray).italic(),
                )));
            }
            DisplayRole::User => {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    " You ",
                    Style::default().fg(Color::Black).bg(Color::Green).bold(),
                )));
                for line in msg.content.lines() {
                    lines.push(Line::from(format!("  {}", line)));
                }
            }
            DisplayRole::Assistant => {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    " Assistant ",
                    Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
                )));
                for line in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(Color::White),
                    )));
                }
            }
            DisplayRole::Tool => {
                let name = msg.tool_name.as_deref().unwrap_or("Tool");
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  [{name}] "),
                        Style::default().fg(Color::Magenta).bold(),
                    ),
                    Span::styled(
                        truncate_content(&msg.content, max_w.saturating_sub(20)),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }
    }

    // Auto-scroll: show the last N lines that fit
    let visible_height = area.height as usize;
    let skip = if lines.len() > visible_height {
        lines.len() - visible_height
    } else {
        0
    };

    let paragraph = Paragraph::new(lines)
        .scroll((skip as u16, 0));
    frame.render_widget(paragraph, area);
}

fn render_separator(frame: &mut Frame, area: Rect) {
    let sep = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(sep, area);
}

fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    let prompt = if app.is_loading { " ... " } else { " > " };
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(if app.is_loading {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Blue)
        })
        .title(prompt);

    let input_text = Paragraph::new(app.input.as_str()).block(input_block);

    frame.render_widget(input_text, area);

    // Cursor position
    if !app.is_loading {
        frame.set_cursor_position((
            area.x + app.cursor_pos as u16 + 1,
            area.y + 1,
        ));
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let status = Paragraph::new(app.status.as_str())
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));
    frame.render_widget(status, area);
}

fn render_permission_dialog(frame: &mut Frame, dialog: &super::app::PermissionDialog) {
    let area = centered_rect(60, 30, frame.area());

    // Clear background
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Permission Required ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let text = vec![
        Line::from(format!("Tool: {}", dialog.tool_name)),
        Line::from(""),
        Line::from(dialog.description.as_str()),
        Line::from(""),
        Line::from(vec![
            if dialog.selected {
                Span::styled("[Allow]", Style::default().fg(Color::Green).bold())
            } else {
                Span::styled(" Allow ", Style::default().fg(Color::DarkGray))
            },
            Span::raw("  "),
            if !dialog.selected {
                Span::styled("[Deny]", Style::default().fg(Color::Red).bold())
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
        format!("{} [+{} lines]", first_line, content.lines().count() - 1)
    } else {
        first_line.to_string()
    }
}

/// Helper to create a centered rectangle
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
