# Live Activity Display Design Spec

**Date:** 2026-04-07
**Goal:** Show real-time tool activity summary, current file, and streaming status matching original Claude Code UX.

## Overview

Replace individual "● ToolName / ⎿ Running…" lines with aggregated activity summary during tool execution, and add streaming response status with elapsed time and token count.

**Modified files:**
- `src/tui/app.rs` — ActivitySummary struct, response timing fields
- `src/tui/ui.rs` — render_conversation loading section

---

## 1. Activity Summary

### Data Structure

```rust
#[derive(Debug, Clone, Default)]
pub struct ActivitySummary {
    pub reads: Vec<String>,    // file paths
    pub searches: Vec<String>, // patterns (Grep, Glob)
    pub edits: Vec<String>,    // file paths (Edit, Write)
    pub commands: Vec<String>, // command text (Bash)
    pub agents: Vec<String>,   // agent descriptions
    pub skills: Vec<String>,   // skill names
    pub other: Vec<String>,    // anything else
    pub last_detail: String,   // most recent input_summary for └ line
}
```

### Tool → Category Mapping

| Tool Name | Category | Display Text |
|-----------|----------|-------------|
| Read | reads | reading N files |
| Grep, Glob | searches | searching for N patterns |
| Edit, Write | edits | editing N files |
| Bash | commands | running command |
| Agent | agents | running agent |
| Skill | skills | loading skill |
| * | other | N tools |

### Summary Text Generation

Combine active categories into one line:
- Single category: `Searching for 2 patterns…`
- Multiple: `Searching for 2 patterns, reading 3 files…`
- With command: `Running command, reading 1 file…`

Format: join category descriptions with `, `, add `…` at end.

### Lifecycle

- `ToolCallStart { name, .. }` → categorize by name, add to appropriate vec in `ActivitySummary`
  - Use `ToolCallInputReady { name, input }` for the detail (input_summary), update `last_detail`
- `ToolCallComplete { id, .. }` → find and remove from summary by tool call id
  - Need to track `(id, category)` mapping to know which vec to remove from
- When all vecs empty → summary is None (no active tools)

### ID Tracking

Add a parallel map to track which tool call belongs to which category:

```rust
pub active_tool_ids: HashMap<String, ToolCategory>,
```

Where `ToolCategory` is an enum matching the categories above.

On `ToolCallStart`: insert `(id, category)` into map, increment category.
On `ToolCallComplete`: lookup category by id, decrement.

Simpler approach — just count per category instead of tracking individual entries:

```rust
pub struct ActivitySummary {
    pub read_count: u32,
    pub search_count: u32,
    pub edit_count: u32,
    pub command_count: u32,
    pub agent_count: u32,
    pub skill_count: u32,
    pub other_count: u32,
    pub last_detail: String,
    pub tool_categories: HashMap<String, ToolCategory>, // id → category
}
```

On Start: `tool_categories.insert(id, cat); cat_count += 1;`
On Complete: `if let Some(cat) = tool_categories.remove(&id) { cat_count -= 1; }`

### Rendering

In `render_conversation`, replace the current "running tool" individual lines with:

```
  ● {summary_text}
    └ {last_detail}
```

Only show when `activity.total_active() > 0`. Individual running tools are NOT shown separately — they're aggregated here.

Completed tools still render individually (or grouped if 2+).

---

## 2. Streaming Response Status

### Data Fields (in App)

```rust
pub response_start_time: Option<Instant>,
pub response_token_count: u64,
```

### Lifecycle

- First `TextDelta` received → if `response_start_time.is_none()`, set to `Instant::now()`
- `Usage { input_tokens, output_tokens }` → `response_token_count += output_tokens`
- `ResponseComplete` → reset both to None/0

### Rendering

When `is_loading` and text is streaming (`response_start_time.is_some()`):

```
  ✱ Responding… (32s · ↑ 1.2k tokens)
```

- `✱` in yellow/orange (`Color::Yellow`)
- Time format: `<60s` → `Ns`, `≥60s` → `Nm Ns`
- Token format: `<1000` → `N tokens`, `≥1000` → `N.Nk tokens`
- `↑` prefix for tokens (upload direction — sending to user)

When `is_loading` but no text yet (pre-streaming, no active tools):

```
  ✱ Thinking…
```

### Combined Display

The activity summary and streaming status can coexist:

```
  ● Running command…
    └ cargo test --lib
  ✱ Responding… (1m 5s · ↑ 2.3k tokens)
```

Activity summary is above, streaming status is below. Both in the "loading" section at the end of conversation.

---

## 3. Integration with Existing Code

### What Changes in ui.rs

The current loading spinner block (after all messages, `if app.is_loading && !has_in_progress_tool`):

```rust
// REMOVE: old spinner
if app.is_loading && !has_in_progress_tool {
    lines.push(...spinner...);
}
```

Replace with two sections:

1. **Activity summary** — shown when `app.activity.total_active() > 0`
2. **Streaming status** — shown when `app.is_loading` (replaces old spinner entirely)

### What Changes in app.rs

- Add `ActivitySummary` struct and `activity: ActivitySummary` field to `App`
- Add `response_start_time: Option<Instant>` and `response_token_count: u64`
- In `handle_query_event`:
  - `ToolCallStart` → `activity.add(id, name)`
  - `ToolCallInputReady` → `activity.last_detail = summary`
  - `ToolCallComplete` → `activity.remove(id)`
  - `TextDelta` → set `response_start_time` if None
  - `Usage` → add to `response_token_count`
  - `ResponseComplete` → reset `response_start_time`, `response_token_count`

### Running Tool Lines

Currently, individual running tools render with `● ToolName / ⎿ Running…`. These should be SUPPRESSED when the activity summary is active. Only completed tools render individually.

In the ToolCall rendering arm, skip rendering if `result.is_none()` — the activity summary handles it.

---

## 4. Tool Category Classification

```rust
fn classify_tool(name: &str) -> ToolCategory {
    match name {
        "Read" => ToolCategory::Read,
        "Grep" | "Glob" => ToolCategory::Search,
        "Edit" | "Write" => ToolCategory::Edit,
        "Bash" => ToolCategory::Command,
        "Agent" => ToolCategory::Agent,
        "Skill" => ToolCategory::Skill,
        _ => ToolCategory::Other,
    }
}
```

---

## Testing Strategy

- **ActivitySummary:** Unit tests for add/remove/format_summary
- **classify_tool:** Unit tests for each tool mapping
- **Time/token formatting:** Unit tests for edge cases (0s, 59s, 60s, 999 tokens, 1000 tokens)
- **Integration:** Manual TUI testing with tool execution
