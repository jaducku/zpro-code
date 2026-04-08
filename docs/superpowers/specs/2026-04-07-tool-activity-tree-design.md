# Tool Activity Tree Display Design

Improve TUI tool/agent activity display to match the original Claude Code UI: nested tool trees, running status, +N more collapsing, full argument display, and improved status bar.

## Problem

- Running tools are invisible (hidden until result arrives)
- No parent-child relationship between agents and their tools
- Tool arguments shown as short summaries, not full commands
- Status bar shows only "Thinking"/"Working" with no detail
- No "+N more tool uses" collapsing for busy agents

## Reference UI (Original Claude Code)

```
● Explore(Explore TUI display and thinking)
  ├─ Bash(find /Users/zpro/... -name "engine.rs" -o -name "*event*" | head -10)
  │    Running...
  ├─ Bash(find ... | grep -E "(engine|event)" | grep -v target)
  │    Running...
  ├─ Read(src/query/engine.rs)
  +5 more tool uses (ctrl+o to expand)
  (ctrl+b to run in background)

✦ Exploring TUI context… (59s · ↓ 918 tokens)
```

---

## Section 1: Data Model Changes

### DisplayMessage::ToolCall — new fields

```rust
ToolCall {
    id: String,
    name: String,
    input_summary: String,          // short: "engine.rs"
    input_full: Option<String>,     // NEW: full args: "find /Users/... -name engine.rs"
    result: Option<String>,
    is_error: bool,
    collapsed: bool,
    parent_id: Option<String>,      // NEW: agent_id if this tool runs inside an agent
}
```

### DisplayMessage::Agent — new fields

```rust
Agent {
    agent_id: String,
    agent_type: String,               // NEW: "Agent", "Explore", "Skill", etc.
    description: String,
    status: AgentStatus,
    result: Option<String>,
    collapsed: bool,
    tool_count: u32,
    token_count: u64,
    duration_secs: u64,
    child_tool_ids: Vec<String>,      // NEW: tool IDs executed by this agent
    visible_child_count: usize,       // NEW: how many children to show (default 3)
}
```

The `agent_type` determines the header prefix:
- `"Agent"` → `Agent(Research Gemma4 tags)`
- `"Explore"` → `Explore(Explore TUI display)`
- `"Skill"` → `Skill(superpowers:brainstorming)`

Derived from `subagent_type` in AgentTool or `query_source` for skills.
```

### QueryEvent::ToolCallStart — new field

```rust
ToolCallStart {
    id: String,
    name: String,
    parent_agent_id: Option<String>,  // NEW: which agent spawned this tool
}
```

---

## Section 2: Tool Argument Display

### `tool_input_full()` function (tool_summary.rs)

Generates full argument string per tool type:

| Tool | Format | Example |
|------|--------|---------|
| Read | `Read(file_path)` | `Read(/Users/zpro/.../engine.rs)` |
| Bash | `Bash(command)` | `Bash(find ... -name "*.rs" \| head -10)` |
| Edit | `Edit(file_path)` | `Edit(src/tui/app.rs)` |
| Grep | `Grep("pattern" in path)` | `Grep("ThinkingDelta" in src/)` |
| Glob | `Glob(pattern)` | `Glob(**/*.rs)` |
| Write | `Write(file_path)` | `Write(src/new_file.rs)` |
| Agent | `Agent(description)` | `Agent(Research Gemma4 tags)` |
| Other | `ToolName(first 100 chars of JSON)` | `WebSearch({"query": "..."})` |

### Truncation

If full args exceed terminal width: truncate with `...`
```
Bash(find /Users/zpro/02_dev/02_oss/cc-workspace/atom-code-rs/src -name "*.rs" | gr...)
```

### Header display

Tool header shows full args by default (matching original):
```
├─ Bash(find /Users/zpro/... -name "engine.rs" -o -name "*event*" | head -10)
```

Clicking toggles collapsed → shows full result below.

---

## Section 3: Running Status + "+N more"

### Running tools now visible

Currently `result.is_none()` tools are hidden. Change: render them with "Running..." status.

```
├─ Bash(find ... -name "engine.rs" | head -10)
│    Running...
├─ Read(src/query/engine.rs)
│    Running...
```

- "Running..." rendered in Yellow
- On completion: replaced by result first line or "✓ Done"

### "+N more tool uses" collapsing

When an Agent has more child tools than `visible_child_count` (default 3):

```
● Explore(Explore TUI display and thinking)
  ├─ Bash(find ... -name "engine.rs")
  ├─ Read(src/query/engine.rs)
  ├─ Read(src/tui/app.rs)
  +5 more tool uses (ctrl+o to expand)
```

**Toggle behavior:**
- `ctrl+o` or click on "+N more" line → expand all children (`visible_child_count = total`)
- Again → collapse back to default 3
- Agent's own `collapsed` = true → hide all children entirely

### Independent tools (no parent)

Tools with `parent_id: None` render as before — standalone ToolCall messages. No change.

---

## Section 4: Status Bar Improvement

### Current

```
⠋ Thinking… (5s)
```

### New format

```
✦ {activity_label}… ({elapsed} · ↓ {output_tokens} tokens)
```

### activity_label resolution (priority order)

1. `loading_label` if set ("Compacting...", "Loading skill...")
2. Active agent → agent description ("Exploring TUI context")
3. Active tools → `ActivitySummary.format_summary()` ("Reading 2 files, searching")
4. LLM streaming → "Responding"
5. LLM waiting → "Thinking"

### Token display

- `↓ {output_tokens}` from cumulative `Usage` events
- Format: `918 tokens`, `1.2k tokens`, `15.3k tokens`

### Icon

- `✦` prefix (matches original)
- Braille spinner animation preserved

---

## Section 5: Event Flow

### Tool inside an Agent

```
engine.rs: tool execution with agent context
  → QueryEvent::ToolCallStart { id, name, parent_agent_id: Some(agent_id) }
  
app.rs: handle_query_event()
  → Create ToolCall with parent_id = parent_agent_id
  → Find Agent by agent_id, push id to child_tool_ids
  → activity.add(&id, &name)

app.rs: on ToolCallInputReady
  → Update ToolCall.input_full via tool_input_full()
  → Update ToolCall.input_summary via tool_input_summary() (existing)

app.rs: on ToolCallComplete
  → Update ToolCall.result
  → activity.remove(&id)

ui.rs: render
  → Agent renders its child tools inline (not as separate messages)
  → Child tools with parent_id are skipped in top-level message render
```

### Tool without Agent (standalone)

Same as current flow. `parent_agent_id: None` → renders as independent ToolCall.

---

## Section 6: File Changes

| File | Changes |
|------|---------|
| `src/query/types.rs` | `ToolCallStart.parent_agent_id: Option<String>` |
| `src/query/engine.rs` | Pass agent context in ToolCallStart event |
| `src/tui/app.rs` | DisplayMessage fields (input_full, parent_id, child_tool_ids, visible_child_count), event handler updates |
| `src/tui/tool_summary.rs` | `pub fn tool_input_full(name, input) -> String` |
| `src/tui/ui.rs` | Nested tree rendering, Running state, +N more, status bar format |
| `src/tui/run.rs` | ctrl+o keybinding for +N more toggle, hit area for +N more line |

### Not changed

- Tool execution logic (engine.rs tool loop)
- Agent tool implementation (agent_tool.rs)
- Existing collapse/expand for ToolCall results and Agent results
