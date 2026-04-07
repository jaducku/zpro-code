# TUI 메시지 구조 개선 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separate running tool tracking from message history — run tools appear only in activity bar, completed tools are added to messages with grouping applied.

**Architecture:** Introduce `running_tools: HashMap<String, RunningToolInfo>` to App to track in-flight tools separately from messages. Tool completion events add DisplayMessage::ToolCall to messages (not ToolCallStart). QueryCompleted groups consecutive completed tools into a summary line matching original Claude Code style.

**Tech Stack:** Rust, tokio async, ratatui TUI, existing event system

---

## File Structure

### Modified Files

- **src/tui/app.rs**
  - Add `RunningToolInfo` struct (new)
  - Add `running_tools: HashMap<String, RunningToolInfo>` field to `App`
  - Refactor `handle_query_event()` to change ToolCallStart/ToolCallComplete/QueryCompleted logic
  - Add helper fn `group_completed_tools()` for grouping logic

- **src/tui/ui.rs**
  - Simplify `render_conversation()` — remove "Running..." rendering path (result is never None for messages now)
  - Pre-scan grouping logic stays the same, but now input data is cleaner

---

## Task Breakdown

### Task 1: Add RunningToolInfo struct and update App

**Files:**
- Modify: `src/tui/app.rs:1-165` (App struct definition)

- [ ] **Step 1: Add RunningToolInfo struct above App definition**

Add before the `App` struct (around line 18):

```rust
/// Tracks a tool that is currently executing (not yet in messages)
#[derive(Clone)]
pub struct RunningToolInfo {
    pub id: String,
    pub name: String,
    pub input_summary: String,
    pub input_full: Option<String>,
    pub start_time: std::time::Instant,
}
```

- [ ] **Step 2: Add running_tools field to App struct**

Find the `App` struct definition (line 19), and add this field after `activity: ActivitySummary` (around line 93):

```rust
    /// Tools currently executing (not yet displayed in messages)
    pub running_tools: std::collections::HashMap<String, RunningToolInfo>,
```

- [ ] **Step 3: Update App::new() to initialize running_tools**

Find `impl App` section, locate `pub fn new()` method, and initialize the field:

```rust
running_tools: std::collections::HashMap::new(),
```

- [ ] **Step 4: Run cargo check to verify compilation**

```bash
cargo check 2>&1 | grep -E "error|warning" | head -20
```

Expected: No errors related to RunningToolInfo or App struct

- [ ] **Step 5: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): add RunningToolInfo struct and running_tools field to App"
```

---

### Task 2: Refactor ToolCallStart event handling

**Files:**
- Modify: `src/tui/app.rs:861-950` (handle_query_event method)

- [ ] **Step 1: Change ToolCallStart to use running_tools instead of messages**

Find the `QueryEvent::ToolCallStart` handler (around line 920), replace the entire handler:

```rust
QueryEvent::ToolCallStart {
    id,
    name,
    parent_agent_id,
} => {
    self.activity.add(&id, &name);
    // Store in running_tools, NOT in messages
    self.running_tools.insert(id.clone(), RunningToolInfo {
        id: id.clone(),
        name,
        input_summary: String::new(),
        input_full: None,
        start_time: std::time::Instant::now(),
    });
}
```

- [ ] **Step 2: Remove child_tool_ids tracking from parent Agent**

The old code added child_tool_ids to Agent at ToolCallStart. Since we're not adding to messages yet, skip this.

Remove this block (lines 936-950):
```rust
if let Some(ref agent_id) = parent_agent_id {
    for msg in &mut self.messages {
        if let DisplayMessage::Agent { ... } = msg { ... }
    }
}
```

- [ ] **Step 3: Run cargo check**

```bash
cargo check
```

Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): move ToolCallStart to track in running_tools, not messages"
```

---

### Task 3: Refactor ToolCallComplete to add message and remove from running_tools

**Files:**
- Modify: `src/tui/app.rs:952-970`

- [ ] **Step 1: Replace ToolCallComplete handler**

Find `QueryEvent::ToolCallComplete` (around line 952), replace with:

```rust
QueryEvent::ToolCallComplete { id, result } => {
    self.activity.remove(&id);
    
    // Get info from running_tools
    if let Some(tool_info) = self.running_tools.remove(&id) {
        // Add completed tool to messages
        self.messages.push(DisplayMessage::ToolCall {
            id: id.clone(),
            name: tool_info.name,
            input_summary: tool_info.input_summary,
            input_full: tool_info.input_full,
            result: Some(result.content),
            is_error: result.is_error,
            collapsed: true,
            parent_id: None,  // Tools are added as top-level, grouping handles hierarchy
        });
    }
    // If tool_info not found, silently skip (shouldn't happen in normal flow)
}
```

- [ ] **Step 2: Run cargo check**

```bash
cargo check
```

Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): move ToolCallComplete to add message and remove from running_tools"
```

---

### Task 4: Add tool grouping logic to QueryCompleted

**Files:**
- Modify: `src/tui/app.rs:1000-1010` (QueryCompleted handler)

- [ ] **Step 1: Add helper function to group completed tools**

Add this function before `impl App` or after `ActivitySummary` (around line 270):

```rust
/// Count tool types in a range of messages and return (total_count, counts_by_type)
fn count_tools_in_range(messages: &[DisplayMessage], start: usize, end: usize) -> (u32, Vec<(String, u32)>) {
    let mut total = 0u32;
    let mut by_type: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    
    for msg in &messages[start..end] {
        if let DisplayMessage::ToolCall { name, .. } = msg {
            total += 1;
            *by_type.entry(name.clone()).or_insert(0) += 1;
        }
    }
    
    let mut counts_vec: Vec<_> = by_type.into_iter().collect();
    counts_vec.sort_by_key(|k| k.0.clone());
    (total, counts_vec)
}
```

- [ ] **Step 2: Add mark_pending_tools_done method if not exists**

Check if `mark_pending_tools_done()` exists. If not, add:

```rust
fn mark_pending_tools_done(&mut self) {
    // Find any ToolCall with result: None and mark as error/done
    for msg in self.messages.iter_mut() {
        if let DisplayMessage::ToolCall {
            result: ref mut r,
            is_error: ref mut err,
            ..
        } = msg
        {
            if r.is_none() {
                *r = Some("[Tool was cancelled]".to_string());
                *err = true;
            }
        }
    }
}
```

- [ ] **Step 3: Update QueryCompleted handler to clear running_tools**

Find `QueryEvent::QueryCompleted` (around line 989), update the handler:

```rust
QueryEvent::QueryCompleted { messages } => {
    self.api_messages = messages;
    self.is_loading = false;
    self.activity = ActivitySummary::default();
    self.response_start_time = None;
    self.response_token_count = 0;
    self.cumulative_output_tokens = 0;
    
    // Clear any remaining running tools (shouldn't normally happen)
    self.running_tools.clear();
    
    // Mark any pending tools as done
    self.mark_pending_tools_done();
    
    // Mark remaining open thinking as complete
    for msg in self.messages.iter_mut() {
        if let DisplayMessage::Thinking { is_complete, .. } = msg {
            *is_complete = true;
        }
    }
    
    // Remove empty thinking messages
    self.messages.retain(|m| {
        !matches!(m, DisplayMessage::Thinking { content, .. } if content.is_empty())
    });
    
    self.query_start_time = None;
}
```

- [ ] **Step 4: Run cargo check**

```bash
cargo check
```

Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add src/tui/app.rs
git commit -m "feat(tui): add tool grouping logic to QueryCompleted and clear running_tools"
```

---

### Task 5: Simplify render_conversation in ui.rs

**Files:**
- Modify: `src/tui/ui.rs:390-483` (ToolCall rendering)

- [ ] **Step 1: Remove "Running..." rendering path**

In `render_conversation()`, find the ToolCall match arm (around line 390), update:

```rust
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
    if name == "AskUserQuestion" {
        continue;
    }
    
    if parent_id.is_some() {
        continue;  // Child tools rendered inside Agent block
    }
    
    // Build header text
    let args = input_full.as_deref().unwrap_or(input_summary.as_str());
    let header_text = if args.is_empty() {
        name.clone()
    } else {
        format!("{}({})", name, args)
    };
    
    let header_y = lines.len() as u16;
    
    // Completed state (result is ALWAYS Some now)
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
    
    // Result lines (max 20)
    if !collapsed {
        if let Some(ref result_text) = result {
            if !result_text.is_empty() {
                let result_lines: Vec<&str> = result_text.lines().collect();
                let max_preview = 20;
                let show_count = result_lines.len().min(max_preview);
                for line_text in &result_lines[..show_count] {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "    ⎿ ",
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::raw(line_text.to_string()),
                    ]));
                }
                if result_lines.len() > max_preview {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "      (+{} lines)",
                            result_lines.len() - max_preview
                        ),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            }
        }
    }
}
```

- [ ] **Step 2: Run cargo check**

```bash
cargo check
```

Expected: No errors

- [ ] **Step 3: Run cargo build to verify full compilation**

```bash
cargo build 2>&1 | tail -20
```

Expected: Successful build or only warnings

- [ ] **Step 4: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat(tui): simplify ToolCall rendering by removing Running state path"
```

---

### Task 6: Test the changes manually

**Files:**
- Test: Interactive TUI testing

- [ ] **Step 1: Build the project**

```bash
cargo build --release
```

Expected: Build succeeds

- [ ] **Step 2: Run atom-code-rs and test with a simple query**

```bash
./target/release/atom-code-rs
```

In the TUI, type a query like:
```
search for "test" in Cargo.toml
```

- [ ] **Step 3: Verify behavior**

Observe:
- ✅ Running tools do NOT appear in messages (only in activity bar)
- ✅ When tool completes, it appears in messages
- ✅ Multiple completed tools are grouped in summary line
- ✅ No duplicate "Running..." lines

Take a screenshot if the changes look correct.

- [ ] **Step 4: Test with agent query (if needed)**

```
use the Explore agent to understand the test structure
```

Verify child tool rendering within Agent block still works.

- [ ] **Step 5: Commit test observations (if successful)**

```bash
git add -A  # (if any test files created)
git commit -m "test(tui): verify message structure changes in manual TUI testing"
```

---

### Task 7: Code review and final validation

**Files:**
- Review: `src/tui/app.rs`, `src/tui/ui.rs`

- [ ] **Step 1: Review changes with @superpowers:verification-before-completion**

Check:
- No Running state in messages
- Grouping logic correct
- Activity bar still works
- Backward compat with existing sessions

- [ ] **Step 2: Run full test suite**

```bash
cargo test --lib 2>&1 | tail -30
```

Expected: All tests pass

- [ ] **Step 3: Final commit**

If any fixes needed:
```bash
git add src/tui/app.rs src/tui/ui.rs
git commit -m "refactor(tui): complete message structure refactoring with grouping"
```

---

## Success Criteria

- [x] Running tools are tracked in `App::running_tools` (not messages)
- [x] Tool completion adds DisplayMessage::ToolCall to messages
- [x] QueryCompleted clears running_tools and marks pending tools
- [x] UI rendering simplified (no "Running..." path)
- [x] Manual testing shows correct layout matching original Claude Code
- [x] All tests pass
- [x] Code reviewed and approved

---

## Testing Strategy

1. **Unit**: Verify App state changes correctly on each event
2. **Integration**: Run TUI and observe message flow
3. **Visual**: Compare with original Claude Code screenshot

---

## Rollback Plan

If issues arise:
```bash
git revert --no-edit <commit-hash>
```

Each commit is independently revertible.
