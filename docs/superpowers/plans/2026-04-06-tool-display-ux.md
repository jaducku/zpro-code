# Tool Display UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve tool call and subagent display with input summaries, status icons, mouse click collapse/expand.

**Architecture:** Refactor `DisplayMessage` from struct to enum with `Text`/`ToolCall`/`Agent` variants. Add `ToolCallInputReady` event to pipeline. Render with status icons and collapsible results. Track click targets during render for mouse interaction.

**Tech Stack:** Rust, ratatui, crossterm (mouse events), tokio, serde_json

**Spec:** `docs/superpowers/specs/2026-04-06-tool-display-ux-design.md`

---

### Task 1: Add ToolCallInputReady event to query pipeline

**Files:**
- Modify: `src/query/types.rs:86-116` (QueryEvent enum)
- Modify: `src/query/engine.rs:151-163` (ContentBlockStop handler)

- [ ] **Step 1: Add ToolCallInputReady variant to QueryEvent**

In `src/query/types.rs`, add after `ToolCallComplete`:

```rust
/// Tool call input fully received (for display summary)
ToolCallInputReady {
    id: String,
    name: String,
    input: serde_json::Value,
},
```

- [ ] **Step 2: Emit ToolCallInputReady in engine**

In `src/query/engine.rs`, inside the `ContentBlockStop` branch, after `tool_calls.push(...)` (line ~158), before the `clear()` calls, add:

```rust
let _ = event_tx.send(QueryEvent::ToolCallInputReady {
    id: tool_calls.last().unwrap().id.clone(),
    name: tool_calls.last().unwrap().name.clone(),
    input: tool_calls.last().unwrap().input.clone(),
});
```

- [ ] **Step 3: Add temporary catch-all in handle_query_event**

In `src/tui/app.rs`, `handle_query_event`, the `match event` is exhaustive. Adding a new QueryEvent variant causes a compile error. Add a temporary fallback at the end of the match:

```rust
QueryEvent::ToolCallInputReady { .. } => {
    // Handled in Task 6 after DisplayMessage refactor
}
```

- [ ] **Step 4: Verify build**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles with 0 errors

- [ ] **Step 5: Commit**

```bash
git add src/query/types.rs src/query/engine.rs src/tui/app.rs
git commit -m "feat(query): add ToolCallInputReady event with tool input"
```

---

### Task 2: Create tool_summary module

**Files:**
- Create: `src/tui/tool_summary.rs`
- Modify: `src/tui/mod.rs`

- [ ] **Step 1: Write tests for tool_input_summary**

Create `src/tui/tool_summary.rs` with tests at the bottom:

```rust
/// Extract a human-readable one-line summary from tool input JSON.
pub fn tool_input_summary(name: &str, input: &serde_json::Value) -> String {
    // implementation next step
    String::new()
}

fn truncate_line(s: &str, max: usize) -> String {
    let first_line = s.lines().next().unwrap_or(s);
    if first_line.chars().count() > max {
        let truncated: String = first_line.chars().take(max).collect();
        format!("{}…", truncated)
    } else {
        first_line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_bash_summary() {
        let input = json!({"command": "ls -la"});
        assert_eq!(tool_input_summary("Bash", &input), "ls -la");
    }

    #[test]
    fn test_read_summary() {
        let input = json!({"file_path": "/src/main.rs"});
        assert_eq!(tool_input_summary("Read", &input), "/src/main.rs");
    }

    #[test]
    fn test_grep_summary() {
        let input = json!({"pattern": "fn main"});
        assert_eq!(tool_input_summary("Grep", &input), "fn main");
    }

    #[test]
    fn test_edit_summary() {
        let input = json!({"file_path": "/src/lib.rs", "old_string": "foo", "new_string": "bar"});
        assert_eq!(tool_input_summary("Edit", &input), "/src/lib.rs");
    }

    #[test]
    fn test_agent_summary() {
        let input = json!({"description": "Exploring codebase"});
        assert_eq!(tool_input_summary("Agent", &input), "Exploring codebase");
    }

    #[test]
    fn test_unknown_tool_fallback() {
        let input = json!({"some_field": "some_value"});
        let result = tool_input_summary("UnknownTool", &input);
        assert_eq!(result, "some_value");
    }

    #[test]
    fn test_truncate_line_short() {
        assert_eq!(truncate_line("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_line_long() {
        let long = "a".repeat(100);
        let result = truncate_line(&long, 10);
        assert_eq!(result.chars().count(), 11); // 10 + '…'
        assert!(result.ends_with('…'));
    }

    #[test]
    fn test_truncate_line_cjk() {
        let cjk = "한국어테스트문자열이길다";
        let result = truncate_line(cjk, 5);
        assert_eq!(result, "한국어테스…");
    }

    #[test]
    fn test_truncate_line_multiline() {
        assert_eq!(truncate_line("line1\nline2\nline3", 80), "line1");
    }

    #[test]
    fn test_empty_input() {
        let input = json!({});
        assert_eq!(tool_input_summary("Bash", &input), "");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p zpro-code tool_summary -- --nocapture 2>&1 | tail -10`
Expected: most tests FAIL (tool_input_summary returns empty string)

- [ ] **Step 3: Implement tool_input_summary**

Replace the stub body:

```rust
pub fn tool_input_summary(name: &str, input: &serde_json::Value) -> String {
    match name {
        "Bash" => input["command"].as_str()
            .map(|c| truncate_line(c, 80))
            .unwrap_or_default(),
        "Read" | "Write" => input["file_path"].as_str()
            .unwrap_or("").to_string(),
        "Edit" => input["file_path"].as_str()
            .unwrap_or("").to_string(),
        "Grep" => input["pattern"].as_str()
            .unwrap_or("").to_string(),
        "Glob" => input["pattern"].as_str()
            .unwrap_or("").to_string(),
        "Agent" => input["description"].as_str()
            .unwrap_or("").to_string(),
        "TaskCreate" | "TaskUpdate" => input["subject"].as_str()
            .unwrap_or("").to_string(),
        "WebFetch" => input["url"].as_str()
            .unwrap_or("").to_string(),
        "WebSearch" => input["query"].as_str()
            .unwrap_or("").to_string(),
        _ => {
            input.as_object()
                .and_then(|o| o.values().find_map(|v| v.as_str()))
                .map(|s| truncate_line(s, 60))
                .unwrap_or_default()
        }
    }
}
```

- [ ] **Step 4: Add module to tui/mod.rs**

Add to `src/tui/mod.rs`:
```rust
pub mod tool_summary;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p zpro-code tool_summary -- --nocapture 2>&1 | tail -15`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add src/tui/tool_summary.rs src/tui/mod.rs
git commit -m "feat(tui): add tool_summary module with input summary extraction"
```

---

### Task 3: Refactor DisplayMessage to enum

**Files:**
- Modify: `src/tui/app.rs:97-110` (DisplayMessage struct, DisplayRole enum)

- [ ] **Step 1: Replace DisplayMessage struct and DisplayRole enum**

In `src/tui/app.rs`, replace:
```rust
pub struct DisplayMessage {
    pub role: DisplayRole,
    pub content: String,
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisplayRole {
    User,
    Assistant,
    System,
    Tool,
}
```

With:
```rust
#[derive(Debug, Clone)]
pub enum DisplayMessage {
    Text {
        role: TextRole,
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        input_summary: String,
        result: Option<String>,
        is_error: bool,
        collapsed: bool,
    },
    Agent {
        agent_id: String,
        description: String,
        status: AgentStatus,
        result: Option<String>,
        collapsed: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextRole { User, Assistant, System }

#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus { Running, Completed, Failed }
```

- [ ] **Step 2: Add HitArea struct and hit_areas field to App**

In `src/tui/app.rs`, add the struct before `App`:
```rust
pub struct HitArea {
    pub y: u16,
    pub message_idx: usize,
}
```

Add field to `App` struct:
```rust
pub hit_areas: Vec<HitArea>,
```

Initialize in `App::new`:
```rust
hit_areas: Vec::new(),
```

- [ ] **Step 3: Migrate App::new welcome messages**

Replace all `DisplayMessage { role: DisplayRole::..., content: ..., tool_name: ... }` in `App::new` (lines ~134-160) with `DisplayMessage::Text { role: TextRole::..., content: ... }`.

- [ ] **Step 4: Migrate handle_query_event**

Rewrite `handle_query_event` in `src/tui/app.rs`. Key changes:

- `TextDelta`: check last message is `DisplayMessage::Text { role: TextRole::Assistant, .. }`, append to it
- `ToolCallStart`: push `DisplayMessage::ToolCall { id, name, input_summary: String::new(), result: None, is_error: false, collapsed: true }`
- Add new arm `ToolCallInputReady`: find ToolCall by id (reverse scan), set `input_summary`
- `ToolCallInputReady`: defer to Task 6 (leave stub `=> {}` from Task 1)
- `ToolCallComplete`: find ToolCall by id (reverse scan), set `result` and `is_error`
- `AgentCompleted`: push `DisplayMessage::Agent { ... }`
- `BriefMessage`: push `DisplayMessage::Text { role: TextRole::Assistant, ... }`
- `Error`: push `DisplayMessage::Text { role: TextRole::System, ... }`
- Auto-scroll: `self.scroll_offset = 0` at top (unchanged)

- [ ] **Step 5: Migrate restore_display_messages**

Update `App::restore_display_messages` to create `DisplayMessage::Text` variants instead of the old struct.

- [ ] **Step 6: Migrate has_conversation check in render**

In `src/tui/ui.rs`, `render_messages` checks `m.role == DisplayRole::User || m.role == DisplayRole::Assistant`. Change to:
```rust
let has_conversation = app.messages.iter().any(|m| matches!(m, DisplayMessage::Text { role: TextRole::User | TextRole::Assistant, .. } | DisplayMessage::ToolCall { .. } | DisplayMessage::Agent { .. }));
```

- [ ] **Step 7: Migrate run.rs call sites**

Replace all ~18 `DisplayMessage { role: DisplayRole::..., content: ..., tool_name: ... }` in `src/tui/run.rs` with `DisplayMessage::Text { role: TextRole::..., content: ... }`. Update the import line too:
```rust
use super::app::{App, DisplayMessage, TextRole, InputMode};
```

- [ ] **Step 8: Build and fix any remaining compile errors**

Run: `cargo build 2>&1`
Fix any remaining references to old `DisplayRole`, `tool_name`, etc.

- [ ] **Step 9: Run full test suite**

Run: `cargo test --workspace 2>&1 | tail -10`
Expected: 577+ passed (same as before, only pre-existing TeamCreate fail)

- [ ] **Step 10: Commit**

```bash
git add src/tui/app.rs src/tui/ui.rs src/tui/run.rs
git commit -m "refactor(tui): DisplayMessage struct → enum with ToolCall/Agent variants"
```

---

### Task 4: Rewrite render_conversation for new display

**Files:**
- Modify: `src/tui/ui.rs:162-218` (render_conversation function)

- [ ] **Step 1: Rewrite render_conversation**

Replace the entire `render_conversation` function. New version handles `Text`, `ToolCall`, and `Agent` variants:

```rust
fn render_conversation(
    lines: &mut Vec<Line<'static>>,
    app: &App,
    _width: u16,
) -> Vec<super::app::HitArea> {
    let mut hit_areas = Vec::new();

    for (i, msg) in app.messages.iter().enumerate() {
        match msg {
            DisplayMessage::Text { role, content } => {
                match role {
                    TextRole::System => {}
                    TextRole::User => {
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
                    TextRole::Assistant => {
                        lines.push(Line::from(""));
                        for line in content.lines() {
                            lines.push(Line::from(Span::styled(
                                format!("  {}", line),
                                Style::default().fg(Color::White),
                            )));
                        }
                    }
                }
            }
            DisplayMessage::ToolCall {
                name, input_summary, result, is_error, collapsed, ..
            } => {
                let icon = match (result.is_some(), *is_error) {
                    (false, _) => {
                        let elapsed = app.start_time.elapsed().as_millis() as usize;
                        let frames = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
                        Span::styled(
                            format!("  {} ", frames[(elapsed / 80) % frames.len()]),
                            Style::default().fg(Color::Cyan),
                        )
                    }
                    (true, false) => Span::styled("  ✓ ", Style::default().fg(Color::Green)),
                    (true, true) => Span::styled("  ✗ ", Style::default().fg(Color::Red)),
                };
                let name_span = Span::styled(
                    format!("{} ", name),
                    Style::default().fg(Color::Black).bg(Color::Magenta).bold(),
                );
                let summary_span = Span::styled(
                    format!(" {}", input_summary),
                    Style::default().fg(Color::White),
                );
                // Record hit area at current line position
                hit_areas.push(super::app::HitArea { y: lines.len() as u16, message_idx: i });
                lines.push(Line::from(vec![icon, name_span, summary_span]));

                // Expanded result
                if !collapsed {
                    if let Some(ref res) = result {
                        let result_lines: Vec<&str> = res.lines().collect();
                        let max_preview = 5;
                        let show = result_lines.len().min(max_preview);
                        for line in &result_lines[..show] {
                            lines.push(Line::from(Span::styled(
                                format!("  │ {}", line),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                        if result_lines.len() > max_preview {
                            lines.push(Line::from(Span::styled(
                                format!("  │ (+{} lines)", result_lines.len() - max_preview),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                    }
                }
            }
            DisplayMessage::Agent {
                description, status, result, collapsed, ..
            } => {
                let icon = match status {
                    AgentStatus::Running => Span::styled("  ◎ ", Style::default().fg(Color::Cyan)),
                    AgentStatus::Completed => Span::styled("  ✓ ", Style::default().fg(Color::Green)),
                    AgentStatus::Failed => Span::styled("  ✗ ", Style::default().fg(Color::Red)),
                };
                let label = Span::styled(
                    "Agent ",
                    Style::default().fg(Color::Black).bg(Color::Cyan).bold(),
                );
                let desc = Span::styled(
                    format!(" {}", description),
                    Style::default().fg(Color::White),
                );
                hit_areas.push(super::app::HitArea { y: lines.len() as u16, message_idx: i });
                lines.push(Line::from(vec![icon, label, desc]));

                if !collapsed {
                    if let Some(ref res) = result {
                        let result_lines: Vec<&str> = res.lines().collect();
                        let max_preview = 5;
                        let show = result_lines.len().min(max_preview);
                        for line in &result_lines[..show] {
                            lines.push(Line::from(Span::styled(
                                format!("  │ {}", line),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                        if result_lines.len() > max_preview {
                            lines.push(Line::from(Span::styled(
                                format!("  │ (+{} lines)", result_lines.len() - max_preview),
                                Style::default().fg(Color::DarkGray),
                            )));
                        }
                    }
                }
            }
        }
    }

    // Loading spinner (global, for non-tool loading states)
    if app.is_loading && !app.messages.iter().rev().any(|m| matches!(m, DisplayMessage::ToolCall { result: None, .. })) {
        let elapsed = app.start_time.elapsed().as_millis() as usize;
        let frames = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
        let spinner = frames[(elapsed / 80) % frames.len()];
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
```

- [ ] **Step 2: Update render_messages to propagate hit_areas**

Change `render_messages` signature and callers to return `Vec<HitArea>`:

```rust
fn render_messages(frame: &mut Frame, area: Rect, app: &App) -> Vec<super::app::HitArea> {
    // ... existing scroll logic ...
    let hit_areas = if !has_conversation {
        render_welcome(&mut lines, app, area.width);
        Vec::new()
    } else {
        render_conversation(&mut lines, app, area.width)
    };
    // ... existing paragraph + scroll indicator ...
    // Adjust hit_area y values for scroll offset
    hit_areas.into_iter().map(|mut ha| {
        // ha.y is the line index in the full content
        // Convert to screen y by subtracting skip and adding area.y
        let screen_y = if (ha.y as usize) >= skip && (ha.y as usize) < skip + visible_height {
            area.y + (ha.y as usize - skip) as u16
        } else {
            u16::MAX // off-screen, won't match any click
        };
        ha.y = screen_y;
        ha
    }).collect()
}
```

Update `render_in_area` to return hit_areas and `render` to return them:

```rust
pub fn render(frame: &mut Frame, app: &App) -> Vec<super::app::HitArea> {
    render_in_area(frame, app, frame.area(), false)
}

pub fn render_in_area(frame: &mut Frame, app: &App, area: Rect, _with_command: bool) -> Vec<super::app::HitArea> {
    // ... layout ...
    let hit_areas = render_messages(frame, chunks[0], app);
    render_input(frame, chunks[1], app);
    render_status_bar(frame, chunks[2], app);
    if let Some(ta) = &app.typeahead {
        render_typeahead(frame, chunks[1], ta);
    }
    hit_areas
}
```

- [ ] **Step 3: Update run.rs to store hit_areas after draw**

In the `terminal.draw()` block in `run.rs`, capture the returned hit_areas:

```rust
let mut frame_hit_areas = Vec::new();
terminal.draw(|frame| {
    frame_hit_areas = ui::render(frame, &app);
    // ... overlay rendering ...
})?;
app.hit_areas = frame_hit_areas;
```

Note: `frame_hit_areas` is declared outside the closure and captured by mutable reference, allowing the closure to write into it.

- [ ] **Step 4: Remove old truncate_content function from ui.rs**

The `truncate_content` function in `ui.rs` is no longer used (tool display now uses `tool_summary`). Delete it.

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles with 0 errors

- [ ] **Step 6: Run tests**

Run: `cargo test --workspace 2>&1 | tail -10`
Expected: 577+ passed

- [ ] **Step 7: Commit**

```bash
git add src/tui/ui.rs src/tui/run.rs
git commit -m "feat(tui): rewrite tool/agent rendering with icons, collapse, hit areas"
```

---

### Task 5: Enable mouse capture and handle click events

**Files:**
- Modify: `src/tui/run.rs:42-46` (terminal setup), `src/tui/run.rs:105-268` (event loop)

- [ ] **Step 1: Enable mouse capture on terminal setup**

In `run_app`, after `execute!(stdout, EnterAlternateScreen)`, add:
```rust
execute!(stdout, crossterm::event::EnableMouseCapture)?;
```

Before `execute!(terminal.backend_mut(), LeaveAlternateScreen)` at the end, add:
```rust
execute!(terminal.backend_mut(), crossterm::event::DisableMouseCapture)?;
```

- [ ] **Step 2: Add mouse event handler in the event loop**

In the `Event::Key(key)` match arm section, add a new arm for mouse events:

```rust
Event::Mouse(mouse_event) => {
    use crossterm::event::{MouseEventKind, MouseButton};
    if let MouseEventKind::Down(MouseButton::Left) = mouse_event.kind {
        let row = mouse_event.row;
        if let Some(ha) = app.hit_areas.iter().find(|h| h.y == row) {
            let idx = ha.message_idx;
            if let Some(msg) = app.messages.get_mut(idx) {
                match msg {
                    DisplayMessage::ToolCall { collapsed, .. } => {
                        *collapsed = !*collapsed;
                    }
                    DisplayMessage::Agent { collapsed, .. } => {
                        *collapsed = !*collapsed;
                    }
                    _ => {}
                }
            }
        }
    }
}
```

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: compiles with 0 errors, 0 warnings

- [ ] **Step 4: Run tests**

Run: `cargo test --workspace 2>&1 | tail -10`
Expected: 577+ passed

- [ ] **Step 5: Commit**

```bash
git add src/tui/run.rs
git commit -m "feat(tui): enable mouse capture, click-to-toggle tool results"
```

---

### Task 6: Handle ToolCallInputReady in TUI event loop

**Files:**
- Modify: `src/tui/run.rs:271-287` (query event handler)
- Modify: `src/tui/app.rs` (handle_query_event)

- [ ] **Step 1: Add ToolCallInputReady handling in handle_query_event**

In `App::handle_query_event`, add a new match arm:

```rust
QueryEvent::ToolCallInputReady { id, input, name } => {
    // Find the ToolCall by id (reverse scan — it's near the end)
    for msg in self.messages.iter_mut().rev() {
        if let DisplayMessage::ToolCall { id: ref msg_id, ref mut input_summary, .. } = msg {
            if *msg_id == id {
                *input_summary = crate::tui::tool_summary::tool_input_summary(&name, &input);
                break;
            }
        }
    }
}
```

- [ ] **Step 2: Build and verify**

Run: `cargo build 2>&1 | tail -5`
Expected: 0 errors, 0 warnings

- [ ] **Step 3: Run tests**

Run: `cargo test --workspace 2>&1 | tail -10`
Expected: 577+ passed

- [ ] **Step 4: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): wire ToolCallInputReady to update tool input summaries"
```

---

### Task 7: Final integration test and cleanup

**Files:**
- All modified files

- [ ] **Step 1: Run full build with warnings check**

Run: `cargo build 2>&1 | grep -i warn`
Expected: no warnings

- [ ] **Step 2: Run full test suite**

Run: `cargo test --workspace 2>&1 | tail -10`
Expected: 577+ passed (pre-existing TeamCreate fail only)

- [ ] **Step 3: Verify no unused imports or dead code**

Run: `cargo clippy 2>&1 | head -30`
Fix any clippy warnings in modified files.

- [ ] **Step 4: Final commit if any cleanup**

```bash
git add -A
git commit -m "chore: cleanup warnings and unused code from tool display UX"
```
