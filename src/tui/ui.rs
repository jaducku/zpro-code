use ratatui::prelude::*;
use ratatui::widgets::*;

use super::app::{App, DisplayRole};

// Logo is rendered with colors in render_welcome(), not as a static string

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(frame: &mut Frame, app: &App) {
    render_in_area(frame, app, frame.area());
}

pub fn render_in_area(frame: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::vertical([
        Constraint::Min(1),    // Messages area
        Constraint::Length(3), // Input
        Constraint::Length(1), // Status bar
    ]);
    let chunks = layout.split(area);

    render_messages(frame, chunks[0], app);
    render_input(frame, chunks[1], app);
    render_status_bar(frame, chunks[2], app);

    // Typeahead overlay (rendered above input area)
    if let Some(ta) = &app.typeahead {
        render_typeahead(frame, chunks[1], ta);
    }

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

fn render_welcome(lines: &mut Vec<Line<'static>>, app: &App, _width: u16) {
    let elapsed = app.start_time.elapsed().as_millis() as usize;
    let colors = [Color::Cyan, Color::LightCyan, Color::Blue, Color::Magenta];
    let ci = (elapsed / 400) % colors.len();

    let m = "  ";

    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // ATOM block logo — left aligned, color gradient
    let logo: &[(&str, usize)] = &[
        (" █████╗ ████████╗ ██████╗ ███╗   ███╗", 0),
        ("██╔══██╗╚══██╔══╝██╔═══██╗████╗ ████║", 0),
        ("███████║   ██║   ██║   ██║██╔████╔██║", 1),
        ("██╔══██║   ██║   ██║   ██║██║╚██╔╝██║", 2),
        ("██║  ██║   ██║   ╚██████╔╝██║ ╚═╝ ██║", 2),
        ("╚═╝  ╚═╝   ╚═╝    ╚═════╝ ╚═╝     ╚═╝", 3),
    ];

    for (text, color_offset) in logo {
        let c = colors[(ci + color_offset) % colors.len()];
        lines.push(Line::from(vec![
            Span::raw(m),
            Span::styled(text.to_string(), Style::default().fg(c).bold()),
        ]));
    }

    lines.push(Line::from(""));

    // Version
    lines.push(Line::from(vec![
        Span::raw(m),
        Span::styled(
            format!("v{}  ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("open LLM coding assistant", Style::default().fg(Color::DarkGray)),
    ]));

    lines.push(Line::from(""));

    // Connection status
    if app.has_provider {
        lines.push(Line::from(vec![
            Span::raw(m),
            Span::styled("* ", Style::default().fg(Color::Green).bold()),
            Span::styled(app.model.clone(), Style::default().fg(Color::Green)),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::raw(m),
            Span::styled("o ", Style::default().fg(Color::DarkGray)),
            Span::styled("no provider", Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines.push(Line::from(""));

    lines.push(Line::from(vec![
        Span::raw(m),
        Span::styled("Enter: send  |  Ctrl+C: quit", Style::default().fg(Color::DarkGray)),
    ]));
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
        // cursor_pos is char index; for display, compute display width of chars before cursor
        let display_offset: u16 = app.input.chars()
            .take(app.cursor_pos)
            .map(|c| if c.is_ascii() { 1 } else { 2 }) // CJK chars are double-width
            .sum();
        frame.set_cursor_position((area.x + display_offset + 1, area.y + 1));
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let left = if app.has_provider {
        format!(" {} ", app.model)
    } else {
        " atom ".into()
    };

    let right = format!(" tokens: {} | ${:.4} ", app.token_count, app.session_cost);

    let total_len = left.len() + right.len();
    let middle_width = (area.width as usize).saturating_sub(total_len);
    let middle = " ".repeat(middle_width);

    let status_line = Line::from(vec![
        Span::styled(&left, Style::default().fg(Color::Cyan).bold().bg(Color::DarkGray)),
        Span::styled(middle, Style::default().bg(Color::DarkGray)),
        Span::styled(&right, Style::default().fg(Color::White).bg(Color::DarkGray)),
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

    let btn = |label: &str, idx: usize, selected: usize, active_bg: Color| -> Span<'static> {
        if selected == idx {
            Span::styled(
                format!(" {} ", label),
                Style::default().fg(Color::Black).bg(active_bg).bold(),
            )
        } else {
            Span::styled(format!(" {} ", label), Style::default().fg(Color::DarkGray))
        }
    };

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Tool: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                dialog.tool_name.clone(),
                Style::default().fg(Color::White).bold(),
            ),
        ]),
        Line::from(""),
        Line::from(format!("  {}", dialog.description)),
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            btn("Allow (y)", 0, dialog.selected, Color::Green),
            Span::raw("  "),
            btn("Always Allow (a)", 1, dialog.selected, Color::Cyan),
            Span::raw("  "),
            btn("Deny (n)", 2, dialog.selected, Color::Red),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_typeahead(frame: &mut Frame, input_area: Rect, ta: &super::app::TypeaheadState) {
    use super::app::TypeaheadKind;

    let max_visible = 10.min(ta.items.len());
    if max_visible == 0 {
        return;
    }

    let height = max_visible as u16 + 2; // +2 for borders

    let (title, prefix, title_color) = match &ta.kind {
        TypeaheadKind::Command => (" Commands ", "/", Color::Cyan),
        TypeaheadKind::FileMention { .. } => (" Files ", "", Color::Yellow),
    };

    // Position above the input area
    let area = Rect {
        x: input_area.x,
        y: input_area.y.saturating_sub(height),
        width: input_area.width.min(70),
        height,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(title, Style::default().fg(title_color)));

    // Scroll window around selected item
    let scroll_start = if ta.selected >= max_visible {
        ta.selected - max_visible + 1
    } else {
        0
    };

    let items: Vec<ListItem> = ta
        .items
        .iter()
        .enumerate()
        .skip(scroll_start)
        .take(max_visible)
        .map(|(i, (name, desc))| {
            let is_selected = i == ta.selected;
            let sel_bg = match &ta.kind {
                TypeaheadKind::Command => Color::Cyan,
                TypeaheadKind::FileMention { .. } => Color::Yellow,
            };
            let name_fg = match &ta.kind {
                TypeaheadKind::Command => Color::Green,
                TypeaheadKind::FileMention { .. } => {
                    if name.ends_with('/') { Color::Blue } else { Color::Yellow }
                }
            };

            let style = if is_selected {
                Style::default().fg(Color::Black).bg(sel_bg).bold()
            } else {
                Style::default()
            };
            let name_style = if is_selected {
                Style::default().fg(Color::Black).bg(sel_bg).bold()
            } else {
                Style::default().fg(name_fg)
            };
            let desc_style = if is_selected {
                Style::default().fg(Color::Black).bg(sel_bg)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let icon = match &ta.kind {
                TypeaheadKind::Command => "",
                TypeaheadKind::FileMention { .. } => {
                    if name.ends_with('/') { "[d] " } else { "[f] " }
                }
            };

            let formatted_name = format!(" {}{}{}", icon, prefix, name);
            let pad = 24_usize.saturating_sub(formatted_name.len());
            let padded = format!("{}{}", formatted_name, " ".repeat(pad));

            let max_desc = (area.width as usize).saturating_sub(padded.len() + 2);
            let truncated_desc = if desc.len() > max_desc {
                format!("{}...", &desc[..max_desc.saturating_sub(3)])
            } else {
                desc.clone()
            };

            ListItem::new(Line::from(vec![
                Span::styled(padded, name_style),
                Span::styled(truncated_desc, desc_style),
            ]))
            .style(style)
        })
        .collect();

    frame.render_widget(Clear, area);
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
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
