use std::collections::HashMap;

use ratatui::prelude::*;
use ratatui::widgets::*;

use super::app::{AgentStatus, App, DisplayMessage, HitArea, TextRole};

/// Cache for parsed markdown: maps message index -> (content length, parsed lines).
/// Re-parses only when content length changes (i.e. streaming appends).
pub type MarkdownCache = HashMap<usize, (usize, Vec<Line<'static>>)>;

// Logo is rendered with colors in render_welcome(), not as a static string

pub fn render(
    frame: &mut Frame,
    app: &App,
    md_cache: &mut MarkdownCache,
) -> (Vec<HitArea>, Vec<HitArea>) {
    render_in_area(frame, app, frame.area(), false, md_cache)
}

/// Compute the overlay area for TUI commands (e.g. /model picker).
/// Positioned above the input area, similar to typeahead.
pub fn compute_command_overlay_area(terminal_area: Rect, content_height: u16) -> Rect {
    // Input is 3 rows + status bar is 1 row = 4 rows from bottom
    let input_top = terminal_area.y + terminal_area.height.saturating_sub(4);
    let max_height = terminal_area.height / 2;
    let height = content_height.min(max_height).max(1);
    let y = input_top.saturating_sub(height);
    let width = terminal_area.width;
    let x = terminal_area.x;

    Rect {
        x,
        y,
        width,
        height,
    }
}

pub fn render_in_area(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    _with_command: bool,
    md_cache: &mut MarkdownCache,
) -> (Vec<HitArea>, Vec<HitArea>) {
    let input_line_count = app.input.matches('\n').count() + 1;
    let max_input_height = (area.height / 3) as usize;
    let input_height = (input_line_count.clamp(1, max_input_height) + 2) as u16; // +2 for border

    let activity_height = compute_activity_height(app);
    let todo_height = if app.todos.is_empty() {
        0
    } else {
        app.todos.len().min(10) as u16 + if app.todos.len() > 10 { 1 } else { 0 }
    };

    let layout = Layout::vertical([
        Constraint::Min(1),                  // Messages area
        Constraint::Length(activity_height), // Activity bar (0 when idle)
        Constraint::Length(todo_height),     // Todo list (0 when empty)
        Constraint::Length(input_height),    // Input (dynamic)
        Constraint::Length(1),               // Status bar
    ]);
    let chunks = layout.split(area);

    let (hit_areas, expand_hit_areas) = render_messages(frame, chunks[0], app, md_cache);
    if activity_height > 0 {
        render_activity(frame, chunks[1], app);
    }
    if todo_height > 0 {
        render_todos(&app.todos, chunks[2], frame.buffer_mut());
    }
    render_input(frame, chunks[3], app);
    render_status_bar(frame, chunks[4], app);

    // Typeahead overlay (rendered above input area)
    if let Some(ta) = &app.typeahead {
        render_typeahead(frame, chunks[3], ta);
    }

    (hit_areas, expand_hit_areas)
}

fn render_messages(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    md_cache: &mut MarkdownCache,
) -> (Vec<HitArea>, Vec<HitArea>) {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Show logo if no conversation yet (only system messages)
    let has_conversation = app.messages.iter().any(|m| {
        matches!(
            m,
            DisplayMessage::Text {
                role: TextRole::User | TextRole::Assistant,
                ..
            } | DisplayMessage::ToolCall { .. }
                | DisplayMessage::Agent { .. }
        )
    });

    let mut hit_areas = Vec::new();
    let mut expand_hit_areas = Vec::new();
    if !has_conversation {
        render_welcome(&mut lines, app, area.width);
    } else {
        (hit_areas, expand_hit_areas) = render_conversation(&mut lines, app, area.width, md_cache);
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

    // Convert expand_hit_area y values from line-index to screen-y
    for ha in &mut expand_hit_areas {
        let line_idx = ha.y as usize;
        if line_idx >= skip && line_idx < skip + visible_height {
            ha.y = area.y + (line_idx - skip) as u16;
        } else {
            ha.y = u16::MAX; // off-screen
        }
    }

    // Clear the message area first to prevent ghost text from previous frames.
    frame.render_widget(Clear, area);
    let paragraph = Paragraph::new(lines).scroll((skip as u16, 0));
    frame.render_widget(paragraph, area);

    // Scrollbar — only shown when content overflows
    if max_scroll > 0 {
        let mut scrollbar_state =
            ScrollbarState::new(max_scroll).position(max_scroll.saturating_sub(effective_offset));
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("┃")
            .track_symbol(Some("│"))
            .thumb_style(Style::default().fg(Color::DarkGray))
            .track_style(Style::default().fg(Color::Rgb(50, 50, 50)));
        frame.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }

    (hit_areas, expand_hit_areas)
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
        (
            "███████╗██████╗ ██████╗  ██████╗        ██████╗ ██████╗ ██████╗ ███████╗",
            0,
        ),
        (
            "╚══███╔╝██╔══██╗██╔══██╗██╔═══██╗      ██╔════╝██╔═══██╗██╔══██╗██╔════╝",
            0,
        ),
        (
            "  ███╔╝ ██████╔╝██████╔╝██║   ██║█████╗██║     ██║   ██║██║  ██║█████╗  ",
            1,
        ),
        (
            " ███╔╝  ██╔═══╝ ██╔══██╗██║   ██║╚════╝██║     ██║   ██║██║  ██║██╔══╝  ",
            2,
        ),
        (
            "███████╗██║     ██║  ██║╚██████╔╝      ╚██████╗╚██████╔╝██████╔╝███████╗",
            2,
        ),
        (
            "╚══════╝╚═╝     ╚═╝  ╚═╝ ╚═════╝       ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝",
            3,
        ),
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
        Span::styled(
            "open LLM coding agent",
            Style::default().fg(Color::DarkGray),
        ),
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
        Span::styled(
            "Enter: send  |  Ctrl+C: quit",
            Style::default().fg(Color::DarkGray),
        ),
    ]));
}

fn render_conversation(
    lines: &mut Vec<Line<'static>>,
    app: &App,
    _width: u16,
    md_cache: &mut MarkdownCache,
) -> (Vec<HitArea>, Vec<HitArea>) {
    let mut hit_areas = Vec::new();
    let mut expand_hit_areas = Vec::new();

    for (msg_idx, msg) in app.messages.iter().enumerate() {
        match msg {
            DisplayMessage::Text {
                role: TextRole::System,
                ..
            } => {}
            DisplayMessage::Text {
                role: TextRole::User,
                content,
            } => {
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
            DisplayMessage::Text {
                role: TextRole::Assistant,
                content,
            } => {
                lines.push(Line::from(""));
                let len = content.len();
                let cached = md_cache
                    .entry(msg_idx)
                    .and_modify(|e| {
                        if e.0 != len {
                            e.0 = len;
                            e.1 = crate::tui::markdown::parse_markdown(content);
                        }
                    })
                    .or_insert_with(|| (len, crate::tui::markdown::parse_markdown(content)));
                for line in &cached.1 {
                    let mut prefixed: Vec<Span<'static>> = vec![Span::raw("  ")];
                    prefixed.extend(line.spans.clone());
                    lines.push(Line::from(prefixed));
                }
            }
            DisplayMessage::ToolCall {
                name,
                input_summary,
                input_full,
                result,
                is_error,
                collapsed,
                parent_id,
                ..
            } => {
                // AskUserQuestion is displayed via Question input UI, not as a ToolCall badge
                if name == "AskUserQuestion" {
                    continue;
                }

                // Child tools are rendered inside their parent Agent block
                if parent_id.is_some() {
                    continue;
                }

                // Build header text: prefer input_full when available
                let args = input_full.as_deref().unwrap_or(input_summary.as_str());
                let header_text = if args.is_empty() {
                    name.clone()
                } else {
                    format!("{}({})", name, args)
                };

                let header_y = lines.len() as u16;

                // Completed state (result is always Some now)
                let header_style = if *is_error {
                    Style::default().fg(Color::Red).bold()
                } else {
                    Style::default().fg(Color::White).bold()
                };
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(header_text, header_style),
                ]));
                hit_areas.push(HitArea {
                    y: header_y,
                    message_idx: msg_idx,
                });

                // Result lines — show only if not collapsed
                if !*collapsed {
                    if let Some(ref result_text) = result {
                        if !result_text.is_empty() {
                            let result_lines: Vec<&str> = result_text.lines().collect();
                            let max_preview = 3;
                            let show_count = result_lines.len().min(max_preview);
                            for line_text in &result_lines[..show_count] {
                                lines.push(Line::from(vec![
                                    Span::styled("    ⎿ ", Style::default().fg(Color::DarkGray)),
                                    Span::raw(line_text.to_string()),
                                ]));
                            }
                            if result_lines.len() > max_preview {
                                let expand_y = lines.len() as u16;
                                lines.push(Line::from(Span::styled(
                                    format!(
                                        "    ... +{} lines (ctrl+o to expand)",
                                        result_lines.len() - max_preview
                                    ),
                                    Style::default().fg(Color::DarkGray),
                                )));
                                expand_hit_areas.push(HitArea {
                                    y: expand_y,
                                    message_idx: msg_idx,
                                });
                            }
                        }
                    }
                }
            }
            DisplayMessage::Thinking {
                content,
                token_count,
                is_complete,
                collapsed,
            } => {
                let token_str = format_tokens(*token_count);

                if !is_complete {
                    // In progress: spinner
                    let spinners = ['◐', '◓', '◑', '◒'];
                    let elapsed = app
                        .query_start_time
                        .map(|t| t.elapsed())
                        .unwrap_or_default();
                    let spinner = spinners[(elapsed.as_millis() / 200) as usize % spinners.len()];
                    lines.push(Line::from(vec![
                        Span::styled(format!("{} ", spinner), Style::default().fg(Color::Yellow)),
                        Span::styled("Thinking… ", Style::default().fg(Color::Yellow)),
                        Span::styled(
                            format!("({})", token_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                } else if *collapsed {
                    // Completed, collapsed (clickable)
                    let header_y = lines.len() as u16;
                    lines.push(Line::from(vec![
                        Span::styled("● ", Style::default().fg(Color::DarkGray)),
                        Span::styled("Thinking ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            format!("({})", token_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                    hit_areas.push(HitArea {
                        y: header_y,
                        message_idx: msg_idx,
                    });
                } else {
                    // Completed, expanded
                    let header_y = lines.len() as u16;
                    lines.push(Line::from(vec![
                        Span::styled("▼ ", Style::default().fg(Color::DarkGray)),
                        Span::styled("Thinking ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            format!("({})", token_str),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                    hit_areas.push(HitArea {
                        y: header_y,
                        message_idx: msg_idx,
                    });
                    // Content lines (max 20)
                    for line in content.lines().take(20) {
                        lines.push(Line::from(vec![
                            Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                            Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)),
                        ]));
                    }
                    let total_lines = content.lines().count();
                    if total_lines > 20 {
                        lines.push(Line::from(Span::styled(
                            format!("  │ ... {} more lines", total_lines - 20),
                            Style::default().fg(Color::DarkGray),
                        )));
                    }
                }
            }
            DisplayMessage::Agent {
                agent_id,
                description,
                status,
                result,
                collapsed,
                tool_count,
                token_count,
                duration_secs,
                child_tool_ids,
                visible_child_count,
                ..
            } => {
                // Status icon — matches original Claude Code style
                let (icon, icon_style) = match status {
                    AgentStatus::Running => ("●", Style::default().fg(Color::Green)),
                    AgentStatus::Completed => ("●", Style::default().fg(Color::Green)),
                    AgentStatus::Failed => ("●", Style::default().fg(Color::Red)),
                };

                // Header: ● [Subagent: agent_id] Agent(description)
                let header_y = lines.len() as u16;
                let subagent_label = format!("[Subagent: {}] ", agent_id);
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(icon, icon_style),
                    Span::raw(" "),
                    Span::styled(
                        subagent_label,
                        Style::default().fg(Color::Blue),
                    ),
                    Span::styled(
                        format!("Agent({})", description),
                        Style::default().fg(Color::White).bold(),
                    ),
                ]));

                hit_areas.push(HitArea {
                    y: header_y,
                    message_idx: msg_idx,
                });

                // Render child tools inline (when not collapsed)
                if !collapsed && !child_tool_ids.is_empty() {
                    let show_count = (*visible_child_count).min(child_tool_ids.len());
                    let remaining = child_tool_ids.len().saturating_sub(show_count);

                    for (ci, child_id) in child_tool_ids.iter().take(show_count).enumerate() {
                        let is_last_visible = ci + 1 == show_count && remaining == 0;
                        let branch = if is_last_visible { "└─" } else { "├─" };

                        // Find the child ToolCall in app.messages
                        let child_msg = app.messages.iter().find(
                            |m| matches!(m, DisplayMessage::ToolCall { id, .. } if id == child_id),
                        );

                        if let Some(DisplayMessage::ToolCall {
                            name,
                            input_summary,
                            input_full,
                            result,
                            ..
                        }) = child_msg
                        {
                            let args = input_full.as_deref().unwrap_or(input_summary.as_str());
                            let child_header = if args.is_empty() {
                                name.clone()
                            } else {
                                format!("{}({})", name, args)
                            };

                            lines.push(Line::from(vec![
                                Span::raw("  "),
                                Span::styled(
                                    format!("{} ", branch),
                                    Style::default().fg(Color::DarkGray),
                                ),
                                Span::styled(name.clone(), Style::default().bold()),
                                Span::styled(
                                    if args.is_empty() {
                                        String::new()
                                    } else {
                                        format!("({})", args)
                                    },
                                    Style::default().fg(Color::DarkGray),
                                ),
                            ]));
                            let _ = child_header; // used above via name/args directly

                            if result.is_none() {
                                // Running child tool
                                lines.push(Line::from(vec![
                                    Span::raw("  │    "),
                                    Span::styled(
                                        "Running...",
                                        Style::default()
                                            .fg(Color::Yellow)
                                            .add_modifier(Modifier::ITALIC),
                                    ),
                                ]));
                            } else if let Some(ref result_text) = result {
                                // Completed child — show first result line only
                                if let Some(first_line) = result_text.lines().next() {
                                    if !first_line.is_empty() {
                                        lines.push(Line::from(vec![
                                            Span::styled(
                                                "  │  ⎿ ",
                                                Style::default().fg(Color::DarkGray),
                                            ),
                                            Span::styled(
                                                first_line.to_string(),
                                                Style::default().fg(Color::DarkGray),
                                            ),
                                        ]));
                                    }
                                }
                            }
                        }
                    }

                    if remaining > 0 {
                        let expand_y = lines.len() as u16;
                        lines.push(Line::from(Span::styled(
                            format!("  +{} more tool uses (ctrl+o to expand)", remaining),
                            Style::default().fg(Color::DarkGray),
                        )));
                        expand_hit_areas.push(HitArea {
                            y: expand_y,
                            message_idx: msg_idx,
                        });
                    }
                }

                // Stats line: └ Done (N tool uses · Xk tokens · Ys)
                if *status != AgentStatus::Running {
                    let status_text = if *status == AgentStatus::Completed {
                        "Done"
                    } else {
                        "Failed"
                    };
                    let token_display = if *token_count >= 1000 {
                        format!("{:.1}k", *token_count as f64 / 1000.0)
                    } else {
                        format!("{}", token_count)
                    };
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled("└ ", Style::default().fg(Color::DarkGray)),
                        Span::styled(
                            format!(
                                "{} ({} tool uses · {} tokens · {}s)",
                                status_text, tool_count, token_display, duration_secs
                            ),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }

                // Expanded result lines (capped at 20)
                if !collapsed {
                    if let Some(ref result_text) = result {
                        let result_lines: Vec<&str> = result_text.lines().collect();
                        let max_preview = 20;
                        let show_count = result_lines.len().min(max_preview);
                        for line_text in &result_lines[..show_count] {
                            lines.push(Line::from(vec![
                                Span::styled("    │ ", Style::default().fg(Color::DarkGray)),
                                Span::styled(
                                    line_text.to_string(),
                                    Style::default().fg(Color::DarkGray),
                                ),
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

    (hit_areas, expand_hit_areas)
}

/// Compute the height needed for the fixed activity bar above the input.
fn compute_activity_height(app: &App) -> u16 {
    if app.is_loading {
        1
    } else {
        0
    }
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000 {
        format!("{:.1}k tokens", tokens as f64 / 1000.0)
    } else {
        format!("{} tokens", tokens)
    }
}

/// Render the fixed activity bar: Thinking (reasoning) or Working (tools running).
fn render_activity(frame: &mut Frame, area: Rect, app: &App) {
    let elapsed_ms = app.start_time.elapsed().as_millis() as usize;
    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let frame_idx = (elapsed_ms / 80) % spinner_frames.len();
    let spinner = spinner_frames[frame_idx];

    let elapsed = app
        .query_start_time
        .map(|s| crate::tui::app::format_elapsed(s))
        .unwrap_or_default();

    let label = app.activity_label();

    let color = if app.activity.total_active() > 0 {
        Color::Cyan
    } else if app.response_start_time.is_some() {
        Color::Green
    } else {
        Color::Yellow
    };

    let token_str = if app.cumulative_output_tokens > 0 {
        format!(" · ↓ {}", format_tokens(app.cumulative_output_tokens))
    } else {
        String::new()
    };

    let status = if !elapsed.is_empty() {
        format!("{}… ({}{})", label, elapsed, token_str)
    } else {
        format!("{}…", label)
    };

    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled(spinner, Style::default().fg(color)),
        Span::raw(" "),
        Span::styled(status, Style::default().fg(color)),
    ]);

    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(vec![line]), area);
}

fn render_todos(todos: &[crate::state::app_state::TodoItem], area: Rect, buf: &mut Buffer) {
    use crate::state::app_state::TodoStatus;

    if todos.is_empty() {
        return;
    }

    let max_display = todos.len().min(10);
    for (i, todo) in todos.iter().take(max_display).enumerate() {
        let is_last = i == max_display - 1 && todos.len() <= 10;
        let prefix = if is_last { "  └" } else { "  ├" };
        let (icon, icon_color) = match todo.status {
            TodoStatus::InProgress => ("■", Color::Yellow),
            TodoStatus::Pending => ("□", Color::DarkGray),
            TodoStatus::Completed => ("✓", Color::Green),
        };
        let text_color = if todo.status == TodoStatus::Completed {
            Color::DarkGray
        } else {
            Color::White
        };
        let line = Line::from(vec![
            Span::styled(format!("{} ", prefix), Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
            Span::styled(todo.content.clone(), Style::default().fg(text_color)),
        ]);
        buf.set_line(area.x, area.y + i as u16, &line, area.width);
    }

    if todos.len() > 10 {
        let remaining = todos.len() - 10;
        let line = Line::from(Span::styled(
            format!("  └ +{} more tasks", remaining),
            Style::default().fg(Color::DarkGray),
        ));
        buf.set_line(area.x, area.y + 10, &line, area.width);
    }
}

fn render_input(frame: &mut Frame, area: Rect, app: &App) {
    use super::app::InputMode;

    match &app.input_mode {
        InputMode::Permission {
            tool_name,
            description,
            ..
        } => {
            let border_style = Style::default().fg(Color::Yellow);
            let title = format!(" {} ", tool_name);
            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(Span::styled(
                    title,
                    Style::default().fg(Color::Yellow).bold(),
                ));

            let desc_line = Line::from(Span::styled(
                format!(
                    " {}",
                    truncate_content(description, area.width.saturating_sub(4) as usize)
                ),
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
                y: area
                    .y
                    .saturating_sub(needed_height.saturating_sub(area.height)),
                width: area.width,
                height: needed_height.max(area.height),
            };

            frame.render_widget(Clear, expanded_area);
            let input_text = Paragraph::new(vec![desc_line, keys_line]).block(input_block);
            frame.render_widget(input_text, expanded_area);
        }
        InputMode::Question {
            question,
            options,
            selected,
            text_input,
            ..
        } => {
            let border_style = Style::default().fg(Color::Blue);
            let title = if question.is_empty() {
                " Question ".to_string()
            } else {
                let max_title = (area.width as usize).saturating_sub(4);
                let q = if question.chars().count() > max_title {
                    let truncated: String =
                        question.chars().take(max_title.saturating_sub(1)).collect();
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
                    y: area
                        .y
                        .saturating_sub(needed_height.saturating_sub(area.height)),
                    width: area.width,
                    height: needed_height.max(area.height),
                };

                frame.render_widget(Clear, expanded_area);
                let input_text = Paragraph::new(content_lines).block(input_block);
                frame.render_widget(input_text, expanded_area);
            }
        }
        InputMode::ExitConfirm {
            worktree_name,
            uncommitted_count,
            selected,
        } => {
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
                y: area
                    .y
                    .saturating_sub(needed_height.saturating_sub(area.height)),
                width: area.width,
                height: needed_height.max(area.height),
            };

            frame.render_widget(Clear, expanded_area);
            let content =
                Paragraph::new(vec![info_line, options_line, help_line]).block(input_block);
            frame.render_widget(content, expanded_area);
        }
        InputMode::Normal => {
            // Border color and title reflect current state
            let border_color = if app.is_loading {
                Color::Yellow
            } else {
                Color::Cyan
            };
            let border_style = Style::default().fg(border_color);
            let title_span = Span::styled(" > ", Style::default().fg(border_color).bold());

            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(border_style)
                .title(title_span);

            let input_text = Paragraph::new(app.input.as_str())
                .style(Style::default().fg(Color::White))
                .block(input_block);

            frame.render_widget(input_text, area);

            // Always show cursor — user can type while loading (input queued)
            {
                let (row, col) = crate::tui::app::cursor_row_col(&app.input, app.cursor_pos);
                let display_col: u16 = app
                    .input
                    .chars()
                    .skip(app.cursor_pos - col)
                    .take(col)
                    .map(|c| if c.is_ascii() { 1u16 } else { 2u16 })
                    .sum();
                frame.set_cursor_position((area.x + display_col + 1, area.y + 1 + row as u16));
            }
        }
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let bg = Style::default().bg(Color::DarkGray);
    let sep = Span::styled(" · ", bg.fg(Color::Rgb(100, 100, 100)));

    // Model name
    let model_name = if app.has_provider {
        app.active_model().to_string()
    } else {
        "atom".into()
    };

    // CWD with ~ substitution
    let cwd_raw = app.cwd.to_string_lossy().to_string();
    let cwd = match std::env::var("HOME") {
        Ok(home) if cwd_raw.starts_with(&home) => {
            format!("~{}", &cwd_raw[home.len()..])
        }
        _ => cwd_raw,
    };

    // Context usage percentage
    let ctx_pct = if app.context_window > 0 {
        ((app.token_count as f64 / app.context_window as f64) * 100.0).min(100.0)
    } else {
        0.0
    };
    let ctx_color = if ctx_pct >= 80.0 {
        Color::Red
    } else if ctx_pct >= 50.0 {
        Color::Yellow
    } else {
        Color::Green
    };

    // Format token count: 12345 → "12.3k"
    let token_display = if app.token_count >= 1_000_000 {
        format!("{:.1}M", app.token_count as f64 / 1_000_000.0)
    } else if app.token_count >= 1_000 {
        format!("{:.1}k", app.token_count as f64 / 1_000.0)
    } else {
        format!("{}", app.token_count)
    };

    // Right section (always shown)
    let right_text = format!(" tokens: {} | ${:.4} ", token_display, app.session_cost);
    let right_span = Span::styled(&right_text, bg.fg(Color::White));
    let right_len = right_text.len();

    // Build left section items: model, perm, cwd, ctx%
    let model_span = Span::styled(format!(" {}", model_name), bg.fg(Color::Cyan).bold());
    let perm_span = Span::styled(app.permission_mode_label.clone(), bg.fg(Color::White));
    let cwd_span = Span::styled(cwd.clone(), bg.fg(Color::White));
    let ctx_text = format!("ctx: {:.0}%", ctx_pct);
    let ctx_span = Span::styled(ctx_text.clone(), bg.fg(ctx_color));

    // Calculate widths for each tier
    let model_len = model_name.len() + 1; // leading space
    let sep_len = 3; // " · "
    let perm_len = app.permission_mode_label.len();
    let cwd_len = cwd.len();
    let ctx_len = ctx_text.len();

    let full_left = model_len + sep_len + perm_len + sep_len + cwd_len + sep_len + ctx_len;
    let no_cwd = model_len + sep_len + perm_len + sep_len + ctx_len;
    let no_perm = model_len + sep_len + ctx_len;
    let _min_left = model_len;

    let avail = (area.width as usize).saturating_sub(right_len);

    let mut left_spans: Vec<Span> = Vec::new();
    if avail >= full_left {
        // Full: model · perm · cwd · ctx%
        left_spans.push(model_span);
        left_spans.push(sep.clone());
        left_spans.push(perm_span);
        left_spans.push(sep.clone());
        left_spans.push(cwd_span);
        left_spans.push(sep.clone());
        left_spans.push(ctx_span);
    } else if avail >= no_cwd {
        // Drop CWD: model · perm · ctx%
        left_spans.push(model_span);
        left_spans.push(sep.clone());
        left_spans.push(perm_span);
        left_spans.push(sep.clone());
        left_spans.push(ctx_span);
    } else if avail >= no_perm {
        // Drop perm: model · ctx%
        left_spans.push(model_span);
        left_spans.push(sep.clone());
        left_spans.push(ctx_span);
    } else {
        // Minimum: model only
        left_spans.push(model_span);
    }

    let left_used: usize = left_spans.iter().map(|s| s.content.len()).sum();
    let pad_width = (area.width as usize).saturating_sub(left_used + right_len);
    let pad = Span::styled(" ".repeat(pad_width), bg);

    let mut spans = left_spans;
    spans.push(pad);
    spans.push(right_span);

    frame.render_widget(Clear, area);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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
                    if name.ends_with('/') {
                        Color::Blue
                    } else {
                        Color::Yellow
                    }
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
                    if name.ends_with('/') {
                        "[d] "
                    } else {
                        "[f] "
                    }
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
