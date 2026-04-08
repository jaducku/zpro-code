# Split-Screen Agent Communication & Tool State Tracking Design

**Date:** 2026-04-08  
**Branch:** `feat/split-screen-agent-communication`  
**Scope:** UI redesign to separate agent communication from task execution monitoring

## Overview

Current status message system shows tool execution inline with conversation. This design introduces a **dual-panel layout** that cleanly separates:

1. **Left Panel (70%)**: Agent communication layer — what agents are saying/thinking
2. **Right Panel (30%)**: Task execution layer — real-time tool call tracking and state updates

The right panel implements **state-based UI architecture** where the display is derived from immutable state changes, not event-driven updates.

## Architecture

### 1. Left Panel: Main Agent Communication

**Display Logic:**
- Show `Assistant` messages from main agent
- Show `Agent` messages from subagents (internal subagent communication)
- User messages and thinking (if expanded) also visible
- Maintain hierarchical relationship: which subagent is which, which agent called which tools

**Key Principle:** Left panel shows full agent communication flow — what agents (main and sub) are saying/doing, in conversation order.

### 2. Right Panel: Main Loop Tool Execution State

**Concept: Main Loop Boundary**
- Starts: User submits a prompt (or agent processes `QueryEvent::UserMessage`)
- Ends: All agents reach idle state (no active tool calls, no running subagents) OR HITL awaiting
- Boundary markers: `/clear` resets the entire state

**Display Structure:**
- One collapsed/expandable section per main loop
- Within each loop: hierarchical tool/agent structure with live state updates
  - Main agent calls
    - └─ Subagent calls (indented)
      - └─ Tools called by subagent (further indented)
    - └─ Direct tool calls by main agent

**State Display (Real-time):**
```
[●] Read file.rs
    ├─ status: Running (elapsed: 2.3s)
    ├─ params: {"path": "/src/main.rs"}
    └─ result: (waiting...)

[✓] Edit file.rs
    ├─ status: Completed (elapsed: 1.1s)
    ├─ params: {"path": "/src/main.rs", "old_string": "...", "new_string": "..."}
    └─ result: "Edit successful"

[✗] Bash (failed)
    ├─ status: Failed (elapsed: 0.8s)
    ├─ params: {"command": "cargo build"}
    └─ error: "Compilation error: ..."
```

**State Updates:** UI re-renders when:
- Tool transitions: Running → Completed/Failed
- Parameters loaded: InputReady event
- Results received: ToolResult event
- Time elapsed: Continuous clock

## State Architecture (Frontend & Backend)

### State Definition (Rust Backend)

```rust
// New struct in src/types/message.rs or dedicated src/types/tool_state.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallState {
    pub tool_use_id: String,
    pub parent_agent_id: Option<String>,  // For hierarchy
    pub parent_tool_use_id: Option<String>, // Already exists in query
    pub tool_name: String,
    pub status: ToolStatus,
    pub created_at: SystemTime,
    pub params: Option<serde_json::Value>,
    pub result: Option<ToolResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolStatus {
    Running,
    Completed(u64), // elapsed_ms
    Failed(String), // error message
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: String, // Truncated to ~500 chars for display
    pub full_content_available: bool,
}

// New struct to track main loop state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainLoopState {
    pub loop_id: String, // UUID or counter
    pub started_at: SystemTime,
    pub status: MainLoopStatus,
    pub tools: Vec<ToolCallState>, // Flat list; hierarchy via parent IDs
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MainLoopStatus {
    Active,
    Idle, // All agents idle, no pending tools
    AwaitingHitl, // HITL input required
}
```

### Frontend State Management

- **State source**: Tauri `invoke` calls OR push events from Rust backend
- **Storage**: Frontend React/Vue state (useState/ref)
- **Rendering**: Derived from `MainLoopState` — no imperative DOM updates
- **Updates**: Event listener receives state deltas, merges into current state, re-renders

**Pseudo-code (React example):**
```typescript
const [mainLoops, setMainLoops] = useState<MainLoopState[]>([]);

// Listen for state updates from Rust
useEffect(() => {
  const unlisten = listen('main-loop-state-updated', (event: { state: MainLoopState }) => {
    setMainLoops(prev => {
      const idx = prev.findIndex(l => l.loop_id === event.state.loop_id);
      if (idx >= 0) {
        const updated = [...prev];
        updated[idx] = event.state; // Immutable update
        return updated;
      }
      return [...prev, event.state];
    });
  });
  return unlisten;
}, []);

// Render
return mainLoops.map(loop => (
  <MainLoopPanel key={loop.loop_id} state={loop} />
));
```

## Data Flow

### 1. Tool Execution Lifecycle (Rust → Frontend)

```
ToolCallStart event (app.rs)
  ↓
Create ToolCallState {status: Running, params: None}
Add to current MainLoopState.tools
Send Tauri event 'tool-state-updated'
  ↓
Frontend receives event
  ↓
Update state: mainLoops[current_loop].tools.push(new_state)
Re-render right panel with new tool (status: Running)
  ↓
[Tool executes...]
  ↓
ToolCallInputReady event
  ↓
Update ToolCallState.params
Send Tauri event
  ↓
Frontend receives, merges params, re-renders (params now visible)
  ↓
[Tool completes...]
  ↓
ToolCallComplete event
  ↓
Update ToolCallState {status: Completed(elapsed_ms), result: ...}
Send Tauri event
  ↓
Frontend receives, updates tool state, re-renders (✓ mark appears)
```

### 2. Main Loop Lifecycle

```
QueryEvent::UserMessage (start of main loop)
  ↓
Create MainLoopState {loop_id, status: Active, tools: []}
  ↓
[Agent processes, calls tools, may invoke subagents...]
  ↓
QueryEvent::ResponseComplete
  ↓
Check: Are all agents idle? (new tracking needed)
  ↓
YES → Update MainLoopState {status: Idle}
NO  → Keep status: Active
  ↓
Send Tauri event 'main-loop-state-updated'
  ↓
Frontend receives, status changes from Active → Idle (right panel shows completed state)
  ↓
User calls /clear
  ↓
Clear all MainLoopState items
Reset right panel
```

## Implementation Details

### Backend Changes (Rust)

**Files to modify/create:**

1. **src/types/message.rs** OR **src/types/tool_state.rs** (new file)
   - Add `ToolCallState`, `ToolStatus`, `ToolResult`, `MainLoopState`, `MainLoopStatus`

2. **src/tui/app.rs** (event handler)
   - Track active main loops: `HashMap<String, MainLoopState>`
   - On `ToolCallStart`: Create `ToolCallState`, add to current loop
   - On `ToolCallInputReady`: Update params in state
   - On `ToolCallComplete`: Update status & result
   - On `ResponseComplete`: Detect idle → Idle state
   - **NEW**: Implement agent idle tracking (see below)

3. **src/services/agent_runtime.rs** OR **src/query/engine.rs**
   - Emit `AgentStateChanged` event with `agent_id` and `is_idle` flag
   - Helps main loop determine when all agents are idle

4. **Tauri IPC (if using invoke):**
   - Add backend command: `get_main_loop_state(loop_id)` → Returns `MainLoopState`
   - Add Tauri event: `main-loop-state-updated` → Pushes `MainLoopState` to frontend

### Agent Idle Tracking (New)

**Problem:** Currently no way to know when "all agents are idle"

**Solution:**
- Maintain `active_agents: HashSet<String>` in `app.rs`
- On `AgentStart`: Add agent_id to set
- On `AgentCompleted`/`AgentFailed`: Remove agent_id from set
- When `ResponseComplete` fires AND `active_agents.is_empty()`: Loop is Idle

### Frontend Changes (TUI or Web)

1. **Right panel layout:**
   - Scrollable container for main loop list
   - Each loop shows:
     - Timestamp (started at)
     - Loop status (Active/Idle/AwaitingHitl)
     - Expandable tool list with hierarchy

2. **Tool state rendering:**
   - Status icon: ● (running/cyan) → ✓ (completed/green) → ✗ (failed/red)
   - Elapsed time: "1.2s" (updated continuously)
   - Tool name & params (truncated)
   - Result (truncated to ~500 chars, "..." for overflow)

3. **State-based updates:**
   - Listen on Tauri event `main-loop-state-updated`
   - Merge state immutably
   - Component re-renders automatically

### Left Panel Changes

1. **Filter `Message` types:**
   - Show: `User`, `Assistant`, `Agent` (all agent messages), `System` (relevant ones), `Thinking` (if expanded)
   - Hide: nested `GroupedToolUse`, internal collapsed structures

2. **Maintain hierarchy:**
   - Show agent relationships via `source_agent_id` (which agent generated this message)
   - Display subagent ID/name in message headers: `[Subagent: planner]` or `[Subagent: code-reviewer]`
   - Main agent messages: no label (or `[Main]`)
   - Natural conversational flow with visible agent context

## Testing

### Unit Tests (Rust)

- `ToolCallState` creation and transitions (Running → Completed/Failed)
- `MainLoopState` management (active agent tracking, loop lifecycle)
- Idle detection logic (when all agents finish)
- Parent-child hierarchy validation (via `parent_agent_id`)

### Integration Tests

- Full tool execution flow: Start → InputReady → Complete → State update
- Main loop lifecycle: UserMessage → Tools run → ResponseComplete → Idle
- Subagent scenario: Main calls subagent → Subagent calls tools → Main receives result
- Multiple sequential loops (one after another)

### UI Tests (Manual or E2E)

- Right panel displays tool execution in real-time
- Hierarchy indentation works correctly (main → subagent → tools)
- Status icon changes (● → ✓)
- Elapsed time increments
- `/clear` resets state

## Success Criteria

- ✅ Right panel shows real-time tool state updates (no delay)
- ✅ Hierarchy is clear (indentation by parent relationship)
- ✅ All tool metadata visible: name, params, status, result, elapsed time
- ✅ Main loop boundaries are clear (starts with user input, ends when all agents idle)
- ✅ Left panel shows full agent communication (main + subagent messages in order)
- ✅ State-based rendering (UI derived from immutable state)
- ✅ `/clear` resets right panel
- ✅ No duplicate state tracking (Single Source of Truth in Rust backend)

## Open Questions / Future Work

- Color scheme for status icons (current: cyan/green/red) — needs design review
- Scrolling behavior when loops exceed visible area (auto-scroll to bottom? pinned header?)
- Should users be able to expand individual tools to see full result (click to expand)?
- Agent idle timeout: How long to wait before considering an agent "idle"? (0ms? or configurable?)
