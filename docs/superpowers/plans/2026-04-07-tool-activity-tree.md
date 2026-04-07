# Tool Activity Tree Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade TUI tool/agent activity display to match original Claude Code UI — nested tool trees, running status, +N more collapsing, full argument display, and improved status bar.

**Architecture:** Bottom-up — event types first, then data model, then rendering. Each layer is testable independently. The key insight: Agent renders its child tools inline, child ToolCalls with `parent_id` are skipped in top-level render.

**Tech Stack:** Rust, ratatui, crossterm

**Spec:** `docs/superpowers/specs/2026-04-07-tool-activity-tree-design.md`

**All commits:** Run `cargo fmt` before each commit. Ignore pre-existing clippy warnings.

---

## Task 1: QueryEvent + DisplayMessage Data Model

**Files:**
- Modify: `src/query/types.rs`
- Modify: `src/tui/app.rs`

- [ ] **Step 1: Add `parent_agent_id` to `ToolCallStart` event**

In `src/query/types.rs`, change:
```rust
ToolCallStart { id: String, name: String },
```
to:
```rust
ToolCallStart { id: String, name: String, parent_agent_id: Option<String> },
```

- [ ] **Step 2: Fix all ToolCallStart construction sites**

Search for `ToolCallStart {` across the codebase. Update each to add `parent_agent_id: None`:
- `src/query/engine.rs` (~line 202): `parent_agent_id: None`
- Any other sites found

- [ ] **Step 3: Fix all ToolCallStart match sites**

Search for `ToolCallStart { id, name }` pattern matches:
- `src/tui/app.rs` (~line 834): change to `ToolCallStart { id, name, parent_agent_id }`
- `src/entrypoints/print.rs` (~line 139): change to `ToolCallStart { name, id, .. }`

- [ ] **Step 4: Add new fields to DisplayMessage::ToolCall**

In `src/tui/app.rs`, change ToolCall variant (lines 129-136):
```rust
ToolCall {
    id: String,
    name: String,
    input_summary: String,
    input_full: Option<String>,     // NEW
    result: Option<String>,
    is_error: bool,
    collapsed: bool,
    parent_id: Option<String>,      // NEW
},
```

- [ ] **Step 5: Add new fields to DisplayMessage::Agent**

In `src/tui/app.rs`, change Agent variant (lines 137-146):
```rust
Agent {
    agent_id: String,
    agent_type: String,             // NEW
    description: String,
    status: AgentStatus,
    result: Option<String>,
    collapsed: bool,
    tool_count: u32,
    token_count: u64,
    duration_secs: u64,
    child_tool_ids: Vec<String>,    // NEW
    visible_child_count: usize,     // NEW
},
```

- [ ] **Step 6: Fix all DisplayMessage construction and match sites**

Every place that creates or destructures `ToolCall { .. }` or `Agent { .. }` needs the new fields. Search for:
- `DisplayMessage::ToolCall {` — add `input_full: None, parent_id: None` (or the real value)
- `DisplayMessage::Agent {` — add `agent_type: "Agent".into(), child_tool_ids: Vec::new(), visible_child_count: 3`

Key sites in `app.rs`:
- ToolCallStart handler (~line 834): set `parent_id: parent_agent_id.clone()`, `input_full: None`
- AgentCompleted handler (~line 946): set `agent_type`, `child_tool_ids: Vec::new()`, `visible_child_count: 3`
- All destructuring in `mark_pending_tools_done`, `handle_cancel`, `restore_display_messages`

Key sites in `ui.rs`:
- ToolCall render block (~line 355)
- Agent render block (~line 417)

Key sites in `run.rs`:
- Mouse click handlers (~lines 505-509)

- [ ] **Step 7: Update ToolCallStart handler to link child tools to agents**

In `app.rs` ToolCallStart handler, after creating the ToolCall message, if `parent_agent_id` is Some, find the Agent message and push the tool id:

```rust
QueryEvent::ToolCallStart { id, name, parent_agent_id } => {
    self.activity.add(&id, &name);
    self.messages.push(DisplayMessage::ToolCall {
        id: id.clone(),
        name: name.clone(),
        input_summary: String::new(),
        input_full: None,
        result: None,
        is_error: false,
        collapsed: true,
        parent_id: parent_agent_id.clone(),
    });
    // Link to parent agent
    if let Some(ref agent_id) = parent_agent_id {
        for msg in &mut self.messages {
            if let DisplayMessage::Agent { agent_id: aid, child_tool_ids, .. } = msg {
                if aid == agent_id {
                    child_tool_ids.push(id.clone());
                    break;
                }
            }
        }
    }
}
```

- [ ] **Step 8: Build and test**

Run: `cargo build 2>&1 | grep "^error" | head -20` → no errors
Run: `cargo test --lib -- 2>&1 | tail -5` → all pass
Run: `cargo fmt`

- [ ] **Step 9: Commit**

```
feat(tui): add parent_id, input_full, agent_type, child_tool_ids to display model
```

---

## Task 2: tool_input_full() Function

**Files:**
- Modify: `src/tui/tool_summary.rs`

- [ ] **Step 1: Write tests**

Add to existing tests in `tool_summary.rs`:

```rust
#[test]
fn test_bash_full() {
    let input = json!({"command": "find /Users/zpro -name '*.rs' | head -10"});
    assert_eq!(
        tool_input_full("Bash", &input),
        "find /Users/zpro -name '*.rs' | head -10"
    );
}

#[test]
fn test_grep_full() {
    let input = json!({"pattern": "ThinkingDelta", "path": "src/"});
    assert_eq!(tool_input_full("Grep", &input), r#""ThinkingDelta" in src/"#);
}

#[test]
fn test_grep_full_no_path() {
    let input = json!({"pattern": "TODO"});
    assert_eq!(tool_input_full("Grep", &input), r#""TODO""#);
}

#[test]
fn test_agent_full() {
    let input = json!({"description": "Research Gemma4 tags"});
    assert_eq!(tool_input_full("Agent", &input), "Research Gemma4 tags");
}

#[test]
fn test_unknown_tool_full() {
    let input = json!({"query": "rust async patterns", "max_results": 5});
    let result = tool_input_full("WebSearch", &input);
    assert!(result.contains("query"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tui::tool_summary -- --nocapture`
Expected: FAIL — `tool_input_full` not found

- [ ] **Step 3: Implement tool_input_full**

```rust
/// Generate full argument string for tool header display.
/// Unlike tool_input_summary (short), this shows complete arguments.
pub fn tool_input_full(name: &str, input: &serde_json::Value) -> String {
    match name {
        "Bash" => input["command"].as_str().unwrap_or("").to_string(),
        "Read" | "Write" => input["file_path"].as_str().unwrap_or("").to_string(),
        "Edit" => input["file_path"].as_str().unwrap_or("").to_string(),
        "Grep" => {
            let pattern = input["pattern"].as_str().unwrap_or("");
            let path = input["path"].as_str();
            match path {
                Some(p) => format!("\"{}\" in {}", pattern, p),
                None => format!("\"{}\"", pattern),
            }
        }
        "Glob" => input["pattern"].as_str().unwrap_or("").to_string(),
        "Agent" => input["description"].as_str().unwrap_or("").to_string(),
        _ => {
            // For unknown tools: compact JSON, truncated to 100 chars
            let json_str = serde_json::to_string(input).unwrap_or_default();
            truncate_line(&json_str, 100)
        }
    }
}
```

- [ ] **Step 4: Wire into ToolCallInputReady handler**

In `src/tui/app.rs`, in the `ToolCallInputReady` handler, after setting `input_summary`, also set `input_full`:

```rust
QueryEvent::ToolCallInputReady { id, input, name } => {
    let summary = crate::tui::tool_summary::tool_input_summary(&name, &input);
    let full = crate::tui::tool_summary::tool_input_full(&name, &input);
    self.activity.last_detail = summary.clone();
    for msg in &mut self.messages {
        if let DisplayMessage::ToolCall {
            id: msg_id,
            input_summary: msg_summary,
            input_full: msg_full,
            ..
        } = msg
        {
            if *msg_id == id {
                *msg_summary = summary;
                *msg_full = Some(full);
                break;
            }
        }
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test --lib tui::tool_summary -- --nocapture` → ALL PASS
Run: `cargo test --lib -- 2>&1 | tail -5` → all pass

- [ ] **Step 6: Commit**

```
feat(tui): add tool_input_full for complete argument display
```

---

## Task 3: Render Running Tools + Nested Tree

**Files:**
- Modify: `src/tui/ui.rs`

This is the largest task — it changes the core rendering logic.

- [ ] **Step 1: Read current ToolCall and Agent render code**

Read `src/tui/ui.rs`:
- ToolCall render (~lines 355-415)
- Agent render (~lines 417-498)
- Message grouping pre-scan (~lines 233-256)
- Understand the `lines` Vec and `hit_areas` pattern

- [ ] **Step 2: Allow running ToolCalls to render**

Currently line ~369: `if result.is_some()` gates ToolCall rendering. Remove this gate for standalone tools (parent_id is None). For child tools (parent_id is Some), they'll be rendered inside the Agent block.

Change the ToolCall render block to:
```rust
DisplayMessage::ToolCall { id, name, input_full, input_summary, result, is_error, collapsed, parent_id } => {
    // Skip child tools — rendered inside their parent Agent
    if parent_id.is_some() {
        continue;
    }
    // Render standalone tool (with or without result)
    let display_name = input_full.as_deref()
        .map(|f| format!("{}({})", name, f))
        .unwrap_or_else(|| format!("{}({})", name, input_summary));
    // ... header line with status icon
    // If result is None: show "Running..." in yellow below
    // If result is Some: show result (existing logic)
}
```

- [ ] **Step 3: Render Agent with inline child tools**

Replace the Agent render block. After the agent header line, iterate `child_tool_ids`, find matching ToolCall messages, render them as tree:

```
● Explore(Explore TUI display and thinking)
  ├─ Bash(find ... -name "engine.rs")        ← child tool, completed
  │  ⎿ file1.rs file2.rs                     ← result (if expanded)
  ├─ Read(src/query/engine.rs)                ← child tool, running
  │    Running...
  +5 more tool uses (ctrl+o to expand)        ← if children > visible_child_count
```

Key rendering logic:
```rust
DisplayMessage::Agent { agent_id, agent_type, description, status, child_tool_ids, visible_child_count, collapsed, .. } => {
    // 1. Header: "● {agent_type}({description})"
    let icon = match status { Running => "●", Completed => "●", Failed => "●" };
    let header = format!("{} {}({})", icon, agent_type, description);
    lines.push(header);
    hit_areas.push(HitArea { y: 0, message_idx: msg_idx });

    if !collapsed {
        // 2. Render child tools
        let visible = visible_child_count.min(child_tool_ids.len());
        for (i, child_id) in child_tool_ids.iter().take(visible).enumerate() {
            // Find the ToolCall message by id
            if let Some(child) = self.messages.iter().find(|m| matches!(m, DisplayMessage::ToolCall { id, .. } if id == child_id)) {
                // Render: "  ├─ ToolName(full_args)"
                // If running: "  │    Running..."
                // If done: "  │  ⎿ result_first_line" (if expanded)
            }
        }
        // 3. "+N more" line
        let remaining = child_tool_ids.len().saturating_sub(visible);
        if remaining > 0 {
            lines.push(format!("  +{} more tool uses (ctrl+o to expand)", remaining));
            // Register hit area for this line
        }
    }
    // 4. Stats line (when completed)
}
```

- [ ] **Step 4: Update message grouping pre-scan**

The pre-scan that groups consecutive completed ToolCalls (~lines 233-256) should skip child tools (`parent_id.is_some()`). Only group standalone completed tools.

- [ ] **Step 5: Build and verify**

Run: `cargo build 2>&1 | grep "^error" | head -20` → no errors
Run: `cargo fmt`

- [ ] **Step 6: Commit**

```
feat(tui): render nested tool tree under agents with running status
```

---

## Task 4: "+N more" Toggle + Hit Areas

**Files:**
- Modify: `src/tui/run.rs`
- Modify: `src/tui/ui.rs` (minor — register hit areas for +N more lines)

- [ ] **Step 1: Add hit area for "+N more" line**

In ui.rs, when rendering the "+N more" line inside Agent, register a HitArea pointing to the Agent message_idx. Use a distinct marker so the click handler knows it's a "+N more" click vs agent header click.

Option: use `message_idx | 0x8000_0000` (high bit set) to distinguish "+N more" clicks from header clicks. Or add a `HitAreaKind` enum.

Simpler approach: add a `pub expand_hit_areas: Vec<HitArea>` to App alongside existing `hit_areas`. "+N more" clicks go to `expand_hit_areas`, header clicks go to `hit_areas`.

- [ ] **Step 2: Handle "+N more" click in run.rs**

In `run.rs` mouse click handler (~line 500-514), add a check for `expand_hit_areas`:

```rust
// Check expand hit areas (+N more toggle)
for ha in &app.expand_hit_areas {
    if mouse_y == ha.y {
        if let Some(DisplayMessage::Agent { visible_child_count, child_tool_ids, .. }) = app.messages.get_mut(ha.message_idx) {
            if *visible_child_count < child_tool_ids.len() {
                *visible_child_count = child_tool_ids.len(); // expand all
            } else {
                *visible_child_count = 3; // collapse back
            }
        }
        break;
    }
}
```

- [ ] **Step 3: Add ctrl+o keybinding**

In `run.rs` key event handler, add `ctrl+o` to toggle the most recent Agent's visible_child_count:

```rust
KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
    // Find last Agent message, toggle its visible_child_count
    for msg in app.messages.iter_mut().rev() {
        if let DisplayMessage::Agent { visible_child_count, child_tool_ids, .. } = msg {
            if *visible_child_count < child_tool_ids.len() {
                *visible_child_count = child_tool_ids.len();
            } else {
                *visible_child_count = 3;
            }
            break;
        }
    }
}
```

- [ ] **Step 4: Build and test**

Run: `cargo build 2>&1 | grep "^error"` → no errors
Run: `cargo test --lib -- 2>&1 | tail -5` → all pass

- [ ] **Step 5: Commit**

```
feat(tui): add +N more toggle via click and ctrl+o
```

---

## Task 5: Status Bar Improvement

**Files:**
- Modify: `src/tui/ui.rs` (status bar render, ~lines 516-550)
- Modify: `src/tui/app.rs` (add `cumulative_output_tokens: u64` field, update on Usage events)

- [ ] **Step 1: Add cumulative token tracking**

In `app.rs` App struct, add:
```rust
pub cumulative_output_tokens: u64,
```

Initialize to 0 in `App::new()`. In `QueryEvent::Usage` handler (~line 895):
```rust
QueryEvent::Usage { input_tokens, output_tokens } => {
    self.response_token_count += output_tokens;
    self.cumulative_output_tokens += output_tokens;
    // ... existing cost tracking
}
```

Reset on QueryCompleted:
```rust
self.cumulative_output_tokens = 0;
```

- [ ] **Step 2: Add activity_label method**

In `app.rs`, add method:
```rust
impl App {
    /// Determine the current activity label for the status bar.
    pub fn activity_label(&self) -> String {
        // 1. Custom loading label
        if let Some(ref label) = self.loading_label {
            return label.clone();
        }
        // 2. Active agent description
        for msg in self.messages.iter().rev() {
            if let DisplayMessage::Agent { description, status: AgentStatus::Running, .. } = msg {
                return description.clone();
            }
        }
        // 3. Active tools summary
        if self.activity.total_active() > 0 {
            return self.activity.format_summary();
        }
        // 4. LLM streaming
        if self.response_start_time.is_some() {
            return "Responding".into();
        }
        // 5. Default
        "Thinking".into()
    }
}
```

- [ ] **Step 3: Update status bar render in ui.rs**

Replace the spinner/status bar render block (~lines 516-550):

```rust
if app.is_loading {
    let elapsed = app.query_start_time.map(|t| t.elapsed()).unwrap_or_default();
    let elapsed_str = format_elapsed(elapsed);
    let label = app.activity_label();

    // Token display
    let token_str = if app.cumulative_output_tokens > 0 {
        format!(" · ↓ {}", format_tokens(app.cumulative_output_tokens))
    } else {
        String::new()
    };

    // Spinner
    let frames = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"];
    let frame = frames[(elapsed.as_millis() / 80) as usize % frames.len()];

    let status_line = format!("{} {}… ({}{})", frame, label, elapsed_str, token_str);
    // Render with appropriate color (Yellow for thinking, Cyan for working, Green for responding)
}
```

- [ ] **Step 4: Add format_tokens helper**

In `ui.rs`:
```rust
fn format_tokens(tokens: u64) -> String {
    if tokens >= 10_000 {
        format!("{:.1}k tokens", tokens as f64 / 1000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k tokens", tokens as f64 / 1000.0)
    } else {
        format!("{} tokens", tokens)
    }
}
```

- [ ] **Step 5: Build and test**

Run: `cargo build 2>&1 | grep "^error"` → no errors
Run: `cargo test --lib -- 2>&1 | tail -5` → all pass

- [ ] **Step 6: Commit**

```
feat(tui): improve status bar with activity label, token count, and elapsed time
```

---

## Task 6: Agent Type from Subagent Context

**Files:**
- Modify: `src/query/types.rs` (add `agent_type` to AgentCompleted event)
- Modify: `src/query/engine.rs` (pass agent_type in event)
- Modify: `src/tui/app.rs` (use agent_type in Agent message)
- Modify: `src/tools/agent_tool.rs` (set agent_type from subagent_type)

- [ ] **Step 1: Add agent_type to QueryEvent::AgentCompleted**

In `src/query/types.rs`:
```rust
AgentCompleted {
    agent_id: String,
    agent_type: String,    // NEW: "Agent", "Explore", "Skill"
    description: String,
    result: Option<String>,
    is_error: bool,
    tool_count: u32,
    token_count: u64,
    duration_secs: u64,
},
```

- [ ] **Step 2: Fix all AgentCompleted construction sites**

Search for `AgentCompleted {` — add `agent_type: "Agent".into()` as default. Then find where actual agent_type is known (agent_tool.rs, skill execution paths) and pass the real value.

- [ ] **Step 3: Use agent_type in app.rs AgentCompleted handler**

```rust
QueryEvent::AgentCompleted { agent_id, agent_type, description, .. } => {
    self.push_message(DisplayMessage::Agent {
        agent_id,
        agent_type,   // Use the real type
        description,
        // ... other fields
    });
}
```

- [ ] **Step 4: Build and test**

Run: `cargo build 2>&1 | grep "^error"` → no errors
Run: `cargo test --lib -- 2>&1 | tail -5` → all pass

- [ ] **Step 5: Commit**

```
feat(tui): pass agent_type through events for Explore/Agent/Skill display headers
```

---

## Task 7: Pass parent_agent_id Through Engine

**Files:**
- Modify: `src/query/engine.rs`
- Modify: `src/tools/agent_tool.rs` (or wherever subagent queries are spawned)

- [ ] **Step 1: Understand how subagent tool events flow**

Read `src/tools/agent_tool.rs` and `src/services/agent_runtime.rs`. Find where the subagent's query emits ToolCallStart events. The subagent runs its own query loop which emits events to an event_tx. These events are relayed to the parent's event_tx.

- [ ] **Step 2: Set parent_agent_id on relayed tool events**

When the agent tool relays a subagent's ToolCallStart event to the parent, set `parent_agent_id` to the agent's own tool_use_id:

```rust
// In the relay loop where subagent events are forwarded:
QueryEvent::ToolCallStart { id, name, .. } => {
    let _ = parent_event_tx.send(QueryEvent::ToolCallStart {
        id,
        name,
        parent_agent_id: Some(agent_tool_use_id.clone()),
    });
}
```

The exact location depends on how `agent_tool.rs` or `agent_runtime.rs` forwards events. Read the code first.

- [ ] **Step 3: Build and test**

Run: `cargo build 2>&1 | grep "^error"` → no errors
Run: `cargo test --lib -- 2>&1 | tail -5` → all pass

- [ ] **Step 4: Commit**

```
feat(tui): thread parent_agent_id through subagent tool events
```

---

## Task 8: Smoke Test

- [ ] **Step 1: Build release**

```bash
cargo build --release 2>&1 | tail -3
```

- [ ] **Step 2: Test standalone tool display**

Launch the app, ask it to read a file. Verify:
- Tool shows immediately with "Running..." status
- On completion, shows result
- Click to expand/collapse result

- [ ] **Step 3: Test agent with nested tools**

Ask a question that triggers an Agent subagent. Verify:
- Agent header shows with proper type (Agent/Explore)
- Child tools appear nested under the agent
- "+N more" appears when > 3 tools
- ctrl+o toggles expand

- [ ] **Step 4: Test status bar**

Verify during a query:
- Shows activity label (not just "Thinking")
- Shows elapsed time
- Shows token count (↓ Nk tokens)

- [ ] **Step 5: Commit**

```
chore: verified tool activity tree display
```
