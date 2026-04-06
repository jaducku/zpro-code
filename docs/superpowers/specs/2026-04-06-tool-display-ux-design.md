# Tool Display UX Design

**Date:** 2026-04-06
**Status:** Approved
**Goal:** Improve tool call and subagent display in TUI — input summary, status icons, collapse/expand with mouse click

## Problem

Current tool display shows a single truncated line per tool call with no input context:
```
 Bash  ls -la output here...
```

No way to see what the tool was asked to do (input), no visual status distinction (running/done/error), no way to expand results, and subagent execution has no dedicated display.

## Design

### 1. Data Model

Replace `DisplayMessage` struct + `DisplayRole` enum with a single enum:

```rust
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
        collapsed: bool,       // default: true
    },
    Agent {
        agent_id: String,
        description: String,
        status: AgentStatus,
        result: Option<String>,
        collapsed: bool,       // default: true
    },
}

pub enum TextRole { User, Assistant, System }
pub enum AgentStatus { Running, Completed, Failed }
```

### 2. Event Pipeline

Add `ToolCallInputReady` event to carry tool input after JSON streaming completes:

```rust
// query/types.rs
pub enum QueryEvent {
    // ... existing ...
    ToolCallInputReady {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}
```

**Emission point:** `src/query/engine.rs` at `ContentBlockStop` — after `tool_calls.push()`, emit `ToolCallInputReady` with the parsed input.

**TUI handling flow:**
1. `ToolCallStart { id, name }` → push `ToolCall { id, name, input_summary: "", result: None }`
2. `ToolCallInputReady { id, name, input }` → find ToolCall by id, set `input_summary` via `tool_input_summary()`
3. `ToolCallComplete { id, result }` → find ToolCall by id, set `result` and `is_error`

### 3. Tool Input Summary

New file `src/tui/tool_summary.rs`:

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
            // Fallback: show first string value from input
            input.as_object()
                .and_then(|o| o.values().find_map(|v| v.as_str()))
                .map(|s| truncate_line(s, 60))
                .unwrap_or_default()
        }
    }
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
```

### 4. Rendering

**Collapsed (default):**
```
  ⠋ Bash  ls -la                     ← running (spinner)
  ✓ Read  src/main.rs                 ← completed (green)
  ✗ Edit  src/foo.rs                  ← error (red)
  ◎ Agent  Exploring codebase         ← agent running (cyan)
  ✓ Agent  Code review                ← agent completed (green)
```

**Expanded (after click):**
```
  ✓ Read  src/main.rs
  │ 1  use std::io;
  │ 2  use std::path::Path;
  │ 3  fn main() {
  │ 4      println!("hello");
  │ 5  }
  │ (+45 lines)
```

**Rendering rules:**
- Icons: `⠋` spinning (Cyan), `✓` done (Green), `✗` error (Red), `◎` agent running (Cyan)
- Tool name: Bold, Magenta background
- Input summary: White, same line after tool name
- Expanded result: `│` prefix, DarkGray, max 5 lines preview + `(+N lines)` if more
- Expanded max height: 20 lines (truncate beyond)

### 5. Mouse Interaction

**Enable:** `crossterm::execute!(stdout, EnableMouseCapture)` in `run_app`, `DisableMouseCapture` on exit.

**Hit area tracking:**

`render_conversation` currently takes `&App` (immutable). To record hit areas during render, return them from the render function instead of mutating App:

```rust
// ui.rs
fn render_conversation(lines: &mut Vec<Line<'static>>, app: &App, width: u16) -> Vec<HitArea> {
    let mut hit_areas = Vec::new();
    // ... when rendering ToolCall/Agent header:
    hit_areas.push(HitArea { y: current_line_y, message_idx: i });
    // ...
    hit_areas
}

pub struct HitArea {
    pub y: u16,
    pub message_idx: usize,
}
```

`render_messages` returns `Vec<HitArea>`, `render`/`render_in_area` returns it out to `run.rs`, which stores it on `app.hit_areas` after `terminal.draw()`.

```rust
// In App:
pub hit_areas: Vec<HitArea>,
```

**Mouse event handling:**
```rust
Event::Mouse(MouseEvent {
    kind: MouseEventKind::Down(MouseButton::Left),
    row, ..
}) => {
    if let Some(ha) = app.hit_areas.iter().find(|h| h.y == row) {
        // toggle collapsed on messages[ha.message_idx]
    }
}
```

### 6. Subagent Display

Subagent events map to `DisplayMessage::Agent`:
- `QueryEvent::AgentCompleted { agent_id, description, result, is_error }` → update existing Agent entry or create new one
- Background agent start: create `Agent { status: Running }` (requires new event or creation at AgentTool execution)
- For now, agents appear on completion (matching current behavior), with `AgentStatus::Completed` or `Failed`

### 7. Migration Notes

**DisplayMessage call sites:** All existing `DisplayMessage { role: DisplayRole::..., content, tool_name }` constructions in `run.rs`, `app.rs` must migrate to `DisplayMessage::Text { role: TextRole::..., content }`. This is mechanical — ~30 call sites, all in `run.rs` and `app.rs`.

**`restore_display_messages`:** `App::restore_display_messages` currently creates `DisplayMessage` structs from `ApiMessage`. Updated to create `DisplayMessage::Text` variants. Tool call history from persisted sessions will display as text (no ToolCall/Agent variants) — acceptable since tool inputs aren't stored in `ApiMessage`.

**ToolCall lookup by id:** `handle_query_event` uses `messages.iter_mut().rev().find()` to locate ToolCall by id. Reverse scan is efficient since the target is always near the end of the vec. No HashMap needed.

**Per-tool spinner:** Use the existing global `app.start_time.elapsed()` for spinner frame calculation — all in-progress tool calls share the same spinner frame. Simpler than per-tool timing.

## File Changes

| File | Changes |
|------|---------|
| `src/tui/app.rs` | `DisplayMessage` enum, `TextRole`, `AgentStatus`, `HitArea`, `hit_areas` field, update `handle_query_event` |
| `src/tui/ui.rs` | `render_conversation` rewrite — icons, collapse/expand, hit_area recording |
| `src/tui/run.rs` | `EnableMouseCapture`/`DisableMouseCapture`, mouse event handler, `ToolCallInputReady` handler |
| `src/tui/mod.rs` | Add `pub mod tool_summary;` |
| `src/tui/tool_summary.rs` | (new) `tool_input_summary()` + `truncate_line()` |
| `src/query/types.rs` | Add `ToolCallInputReady` variant |
| `src/query/engine.rs` | Emit `ToolCallInputReady` at `ContentBlockStop` |

## Success Criteria

- [ ] Tool calls show input summary (command, file path, pattern, etc.)
- [ ] Status icons: spinner while running, ✓ on success, ✗ on error
- [ ] Mouse click toggles collapse/expand on tool results
- [ ] Default state is collapsed
- [ ] Expanded shows max 5 lines preview + "(+N lines)"
- [ ] Subagent completion shows as Agent entries
- [ ] Existing tests pass
- [ ] 0 compiler warnings

## Deferred

- Per-tool result rendering (diff view for Edit, syntax highlight for Read)
- Consecutive same-tool grouping
- Bash real-time output streaming
- Keyboard-based expand/collapse
