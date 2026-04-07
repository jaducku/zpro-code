use ratatui::prelude::*;
use ratatui::widgets::*;

use super::app::{AgentStatus, App, DisplayMessage, HitArea, TextRole};

// Logo is rendered with colors in render_welcome(), not as a static string

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn render(frame: &mut Frame, app: &App) -> Vec<HitArea> {
    render_in_area(frame, app, frame.area(), false)
}

/// Compute the overlay area for TUI commands (e.g. /model picker).
/// Positioned above the input area, similar to typeahead.
pub fn compute_command_overlay_area(terminal_area: Rect, content_height: u16) -> Rect {
    // Input is 3 rows + status bar is 1 row = 4 rows from bottom
    let input_top = terminal_area.y + terminal_area.height.saturating_sub(4);
    let max_height = terminal_area.height / 2;
    let height = content_height.min(max_height).max(1);
    let y = input_top.saturating_sub(height);
    let width = terminal_area.width.min(80);
    let x = terminal_area.x + (terminal_area.width.saturating_sub(width)) / 2;

    Rect { x, y, width, height }
}

pub fn render_in_area(frame: &mut Frame, app: &App, area: Rect, _with_command: bool) -> Vec<HitArea> {
    let layout = Layout::vertical([
        Constraint::Min(1),    // Messages area
        Constraint::Length(3), // Input
        Constraint::Length(1), // Status bar
    ]);
    let chunks = layout.split(area);

    let hit_areas = render_messages(frame, chunks[0], app);
    render_input(frame, chunks[1], app);
    render_status_bar(frame, chunks[2], app);

    // Typeahead overlay (rendered above input area)
    if let Some(ta) = &app.typeahead {
        render_typeahead(frame, chunks[1], ta);
    }

    hit_areas
}

fn render_messages(frame: &mut Frame, area: Rect, app: &App) -> Vec<HitArea> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Show logo if no conversation yet (only system messages)
    let has_conversation = app.messages.iter().any(|m| matches!(m,
        DisplayMessage::Text { role: TextRole::User | TextRole::Assistant, .. }
        | DisplayMessage::ToolCall { .. }
        | DisplayMessage::Agent { .. }
    ));

    let mut hit_areas = Vec::new();
    if !has_conversation {
        render_welcome(&mut lines, app, area.width);
    } else {
        hit_areas = render_conversation(&mut lines, app, area.width);
    }

    // Scroll logic: scroll_offset 0 = bottom (auto-scroll), higher = further up
    let visible_height = area.height as usize;
    let total = lines.len();
    let max_scroll = total.saturating_sub(visible_height);
    let effective_offset = (app.scroll_offset as usize).min(max_scroll);
    let skip = max_scroll.saturating_sub(effective_offset);

    // Convert hit_area y values from line-index to screen-y
    for ha in &mut hit_areas {
        let line_idx = ha.y as usize;
        if line_idx >= skip && line_idx < skip + visible_height {
            ha.y = area.y + (line_idx - skip) as u16;
        } else {
            ha.y = u16::MAX; // off-screen
        }
    }

    let paragraph = Paragraph::new(lines).scroll((skip as u16, 0));
    frame.render_widget(paragraph, area);

    // Scroll indicator when not at bottom
    if effective_offset > 0 {
        let indicator = Span::styled(
            " ↓ more below ",
            Style::default().fg(Color::Black).bg(Color::Cyan),
        );
        let indicator_area = Rect {
            x: area.x + area.width.saturating_sub(16),
            y: area.y + area.height.saturating_sub(1),
            width: 15,
            height: 1,
        };
        frame.render_widget(Paragraph::new(Line::from(indicator)), indicator_area);
    }

    hit_areas
}

fn render_welcome(lines: &mut Vec<Line<'static>>, app: &App, _width: u16) {
    let elapsed = app.start_time.elapsed().as_millis() as usize;
    let colors = [Color::Cyan, Color::LightCyan, Color::Blue, Color::Magenta];
    let ci = (elapsed / 400) % colors.len();

    let m = "  ";

    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // ZPRO-CODE block logo — left aligned, color gradient
    let logo: &[(&str, usize)] = &[
        ("███████╗██████╗ ██████╗  ██████╗        ██████╗ ██████╗ ██████╗ ███████╗", 0),
        ("╚══███╔╝██╔══██╗██╔══██╗██╔═══██╗      ██╔════╝██╔═══██╗██╔══██╗██╔════╝", 0),
        ("  ███╔╝ ██████╔╝██████╔╝██║   ██║█████╗██║     ██║   ██║██║  ██║█████╗  ", 1),
        (" ███╔╝  ██╔═══╝ ██╔══██╗██║   ██║╚════╝██║     ██║   ██║██║  ██║██╔══╝  ", 2),
        ("███████╗██║     ██║  ██║╚██████╔╝      ╚██████╗╚██████╔╝██████╔╝███████╗", 2),
        ("╚══════╝╚═╝     ╚═╝  ╚═╝ ╚═════╝       ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝", 3),
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
        Span::styled("open LLM coding agent", Style::default().fg(Color::DarkGray)),
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

fn render_conversation(lines: &mut Vec<Line<'static>>, app: &App, _width: u16) -> Vec<HitArea> {
    let mut hit_areas = Vec::new();
    let elapsed = app.start_time.elapsed().as_millis() as usize;
    let spinner_idx = (elapsed / 80) % SPINNER_FRAMES.len();

    let mut has_in_progress_tool = false;

    for (msg_idx, msg) in app.messages.iter().enumerate() {
        match msg {
            DisplayMessage::Text { role: TextRole::System, .. } => {}
            DisplayMessage::Text { role: TextRole::User, content } => {
                lines.push(Line::from(""));
                let first = content.lines().next().unwrap_or("").to_string();
                lines.push(Line::from(vec![
                    Span::styled(" > ", Style::default().fg(Color::Green).bold()),
                    Span::styled(first, Style::default().fg(Color::White).bold()),
                ]));
                for line in content.lines().skip(1) {
                    lines.push(Line::from(format!("   {}", line)));
                }
            }
            DisplayMessage::Text { role: TextRole::Assistant, content } => {
                lines.push(Line::from(""));
                for line in content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", line),
                        Style::default().fg(Color::White),
                    )));
                }
            }
            DisplayMessage::ToolCall { name, input_summary, result, is_error, collapsed, .. } => {
                // AskUserQuestion is displayed via Question input UI, not as a ToolCall badge
                if name == "AskUserQuestion" {
                    if result.is_none() {
                        has_in_progress_tool = true;
                    }
                    continue;
                }

                // Status icon
                let (icon, icon_style) = if result.is_none() {
                    has_in_progress_tool = true;
                    (SPINNER_FRAMES[spinner_idx], Style::default().fg(Color::Cyan))
                } else if *is_error {
                    ("✗", Style::default().fg(Color::Red))
                } else {
                    ("✓", Style::default().fg(Color::Green))
                };

                // Header line
                let header_y = lines.len() as u16;
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, icon_style),
                    Span::raw(" "),
                    Span::styled(
                        format!(" {} ", name),
                        Style::default().fg(Color::Black).bg(Color::Magenta).bold(),
                    ),
                    Span::raw(" "),
                    Span::styled(input_summary.clone(), Style::default().fg(Color::DarkGray)),
                ]));

                hit_areas.push(HitArea { y: header_y, message_idx: msg_idx });

                // Expanded result lines
                if !collapsed {
                    if let Some(ref result_text) = result {
                        let result_lines: Vec<&str> = result_text.lines().collect();
                        let max_preview = 5;
                        let show_count = result_lines.len().min(max_preview);
                        for line in &result_lines[..show_count] {
                            lines.push(Line::from(vec![
                                Span::styled("    │ ", Style::default().fg(Color::DarkGray)),
                                Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)),
                            ]));
                        }
                        if result_lines.len() > max_preview {
                            lines.push(Line::from(Span::styled(
                                format!("    (+{} lines)", result_lines.len() - max_preview),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                    }
                }
            }
            DisplayMessage::Agent { description, status, result, collapsed, .. } => {
                // Status icon
                let (icon, icon_style) = match status {
                    AgentStatus::Running => {
                        has_in_progress_tool = true;
                        ("◎", Style::default().fg(Color::Cyan))
                    }
                    AgentStatus::Completed => ("✓", Style::default().fg(Color::Green)),
                    AgentStatus::Failed => ("✗", Style::default().fg(Color::Red)),
                };

                // Header line
                let header_y = lines.len() as u16;
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, icon_style),
                    Span::raw(" "),
                    Span::styled(
                        " Agent ",
                        Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
                    ),
                    Span::raw(" "),
                    Span::styled(description.clone(), Style::default().fg(Color::DarkGray)),
                ]));

                hit_areas.push(HitArea { y: header_y, message_idx: msg_idx });

                // Expanded result lines
                if !collapsed {
                    if let Some(ref result_text) = result {
                        let result_lines: Vec<&str> = result_text.lines().collect();
                        let max_preview = 5;
                        let show_count = result_lines.len().min(max_preview);
                        for line in &result_lines[..show_count] {
                            lines.push(Line::from(vec![
                                Span::styled("    │ ", Style::default().fg(Color::DarkGray)),
                                Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)),
                            ]));
                        }
                        if result_lines.len() > max_preview {
                            lines.push(Line::from(Span::styled(
                                format!("    (+{} lines)", result_lines.len() - max_preview),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                    }
                }
            }
        }
    }

    // Loading spinner: only show if is_loading AND no in-progress ToolCall exists
    if app.is_loading && !has_in_progress_tool {
        let spinner = SPINNER_FRAMES[spinner_idx];
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {} Working...", spinner),
            Style::default().fg(Color::Cyan),
        )));
        if let Some(ref label) = app.loading_label {
            lines.push(Line::from(Span::styled(
                format!("    {}", label),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    hit_areas
}

fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    use super::app::InputMode;

    match &app.input_mode {
        InputMode::Permission { tool_name, description, .. } => {
            let border_style = Style::default().fg(Color::Yellow);
            let title = format!(" {} ", tool_name);
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(Span::styled(title, Style::default().fg(Color::Yellow).bold()));

            let desc_line = Line::from(Span::styled(
                format!(" {}", truncate_content(description, area.width.saturating_sub(4) as usize)),
                Style::default().fg(Color::White),
            ));
            let keys_line = Line::from(vec![
                Span::styled(" [y]", Style::default().fg(Color::Green).bold()),
                Span::styled(" Allow  ", Style::default().fg(Color::White)),
                Span::styled("[n]", Style::default().fg(Color::Red).bold()),
                Span::styled(" Deny  ", Style::default().fg(Color::White)),
                Span::styled("[a]", Style::default().fg(Color::Yellow).bold()),
                Span::styled(" Always allow", Style::default().fg(Color::White)),
            ]);

            // Expand input area to fit description + keys
            let needed_height: u16 = 4; // border + desc + keys + border
            let expanded_area = Rect {
                x: area.x,
                y: area.y.saturating_sub(needed_height.saturating_sub(area.height)),
                width: area.width,
                height: needed_height.max(area.height),
            };

            frame.render_widget(Clear, expanded_area);
            let input_text = Paragraph::new(vec![desc_line, keys_line]).block(input_block);
            frame.render_widget(input_text, expanded_area);
        }
        InputMode::Question { question, options, selected, text_input, .. } => {
            let border_style = Style::default().fg(Color::Blue);
            let title = if question.is_empty() {
                " Question ".to_string()
            } else {
                let max_title = (area.width as usize).saturating_sub(4);
                let q = if question.chars().count() > max_title {
                    let truncated: String = question.chars().take(max_title.saturating_sub(1)).collect();
                    format!("{}…", truncated)
                } else {
                    question.clone()
                };
                format!(" {} ", q)
            };
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(Span::styled(title, Style::default().fg(Color::Blue).bold()));

            if options.is_empty() {
                // Free text input
                let prompt = Line::from(vec![
                    Span::styled("> ", Style::default().fg(Color::Blue).bold()),
                    Span::styled(text_input.clone(), Style::default().fg(Color::White)),
                    Span::styled("_", Style::default().fg(Color::DarkGray)),
                ]);
                let help = Line::from(Span::styled(
                    " Enter to submit, Esc to dismiss",
                    Style::default().fg(Color::DarkGray),
                ));
                let content = vec![prompt, help];
                let input_text = Paragraph::new(content).block(input_block);
                frame.render_widget(input_text, area);
            } else {
                // Options list — one per line, vertically
                let mut content_lines: Vec<Line> = Vec::new();
                for (i, (label, desc)) in options.iter().enumerate() {
                    let marker = if i == *selected { "▸ " } else { "  " };
                    let style = if i == *selected {
                        Style::default().fg(Color::Blue).bold()
                    } else {
                        Style::default().fg(Color::White)
                    };
                    let mut spans = vec![Span::styled(format!(" {}{}", marker, label), style)];
                    if let Some(d) = desc {
                        let desc_style = if i == *selected {
                            Style::default().fg(Color::Blue)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        };
                        spans.push(Span::styled(format!("  {}", d), desc_style));
                    }
                    content_lines.push(Line::from(spans));
                }
                content_lines.push(Line::from(Span::styled(
                    " ↑↓ select  Enter confirm  Esc dismiss",
                    Style::default().fg(Color::DarkGray),
                )));

                // Expand area to fit all options
                let needed_height = (content_lines.len() as u16) + 2; // +2 for borders
                let expanded_area = Rect {
                    x: area.x,
                    y: area.y.saturating_sub(needed_height.saturating_sub(area.height)),
                    width: area.width,
                    height: needed_height.max(area.height),
                };

                frame.render_widget(Clear, expanded_area);
                let input_text = Paragraph::new(content_lines).block(input_block);
                frame.render_widget(input_text, expanded_area);
            }
        }
        InputMode::ExitConfirm { worktree_name, uncommitted_count, selected } => {
            let border_style = Style::default().fg(Color::Red);
            let title = " Exit Confirmation ";
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(Span::styled(title, Style::default().fg(Color::Red).bold()));

            let info = format!(
                " Worktree '{}' has {} uncommitted file(s).",
                worktree_name, uncommitted_count
            );
            let info_line = Line::from(Span::styled(info, Style::default().fg(Color::White)));

            let options = ["Keep & Exit", "Remove & Exit", "Cancel"];
            let mut option_spans = Vec::new();
            for (i, opt) in options.iter().enumerate() {
                if i > 0 {
                    option_spans.push(Span::raw("  "));
                }
                if i == *selected {
                    option_spans.push(Span::styled(
                        format!("[{}]", opt),
                        Style::default().fg(Color::Yellow).bold(),
                    ));
                } else {
                    option_spans.push(Span::styled(
                        format!(" {} ", opt),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
            let options_line = Line::from(option_spans);
            let help_line = Line::from(Span::styled(
                " ←/→ select  Enter confirm  Esc cancel",
                Style::default().fg(Color::DarkGray),
            ));

            let needed_height: u16 = 5; // border + info + options + help + border
            let expanded_area = Rect {
                x: area.x,
                y: area.y.saturating_sub(needed_height.saturating_sub(area.height)),
                width: area.width,
                height: needed_height.max(area.height),
            };

            frame.render_widget(Clear, expanded_area);
            let content = Paragraph::new(vec![info_line, options_line, help_line]).block(input_block);
            frame.render_widget(content, expanded_area);
        }
        InputMode::Normal => {
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
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let left = if app.has_provider {
        format!(" {} ", app.active_model())
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

