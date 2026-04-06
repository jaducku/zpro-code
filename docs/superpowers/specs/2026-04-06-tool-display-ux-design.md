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
    if first_line.len() > max {
        format!("{}…", &first_line[..max])
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
```rust
pub struct HitArea {
    pub y: u16,
    pub message_idx: usize,
}

// In App:
pub hit_areas: Vec<HitArea>,
```

**Flow:**
1. `render_conversation` records `HitArea` for each ToolCall/Agent header line
2. `hit_areas` cleared at start of each frame
3. `Event::Mouse(Down(Left), row, ..)` → find `hit_areas` entry where `y == row` → toggle `collapsed`

### 6. Subagent Display

Subagent events map to `DisplayMessage::Agent`:
- `QueryEvent::AgentCompleted { agent_id, description, result, is_error }` → update existing Agent entry or create new one
- Background agent start: create `Agent { status: Running }` (requires new event or creation at AgentTool execution)
- For now, agents appear on completion (matching current behavior), with `AgentStatus::Completed` or `Failed`

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
