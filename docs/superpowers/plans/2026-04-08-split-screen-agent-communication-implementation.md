# Split-Screen Agent Communication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement dual-panel UI with agent communication on left (70%) and real-time tool state tracking on right (30%), using state-based architecture.

**Architecture:** Backend tracks `MainLoopState` and `ToolCallState` in a single source of truth. Frontend listens on Tauri events for immutable state updates and re-renders automatically. Agent idle detection determines loop boundaries. Left panel shows all agent messages with `[Subagent: xxx]` labels; right panel shows tool hierarchy with real-time status icons.

**Tech Stack:** Rust backend (event-driven), Tauri IPC, Frontend state management (React/Vue), immutable state updates

---

## Phase 1: Backend State Infrastructure

### Task 1: Define ToolCallState & MainLoopState types

**Files:**
- Create: `src/types/tool_state.rs` (new)
- Modify: `src/types/mod.rs`

**Steps:**

- [ ] **Step 1: Create tool_state.rs with types**

```rust
// src/types/tool_state.rs
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Tool call execution state (real-time tracking)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallState {
    pub tool_use_id: String,
    pub parent_agent_id: Option<String>,    // Which agent called this tool
    pub parent_tool_use_id: Option<String>, // For nested tool relationships
    pub tool_name: String,
    pub status: ToolStatus,
    pub created_at: SystemTime,
    pub params: Option<serde_json::Value>,  // Set on InputReady
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
    pub content: String, // Truncated to ~500 chars
    pub full_content_available: bool,
}

/// Main loop state (user input → all agents idle)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainLoopState {
    pub loop_id: String,
    pub started_at: SystemTime,
    pub status: MainLoopStatus,
    pub tools: Vec<ToolCallState>, // Flat list; hierarchy via parent IDs
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MainLoopStatus {
    Active,
    Idle,        // All agents idle, no pending tools
    AwaitingHitl, // HITL input required
}
```

- [ ] **Step 2: Add tool_state module to src/types/mod.rs**

```rust
// In src/types/mod.rs, add:
pub mod tool_state;
pub use tool_state::{ToolCallState, ToolStatus, ToolResult, MainLoopState, MainLoopStatus};
```

- [ ] **Step 3: Run cargo check to verify types compile**

```bash
cargo check
```

Expected: No errors, compiles successfully.

- [ ] **Step 4: Commit types**

```bash
git add src/types/tool_state.rs src/types/mod.rs
git commit -m "types: define ToolCallState and MainLoopState for state tracking"
```

---

### Task 2: Implement agent idle tracking

**Files:**
- Modify: `src/tui/app.rs`
- Modify: `src/query/types.rs` (add event)

**Steps:**

- [ ] **Step 1: Add event type for agent state changes**

In `src/query/types.rs`, add variant to `QueryEvent` enum:

```rust
#[derive(Debug)]
pub enum QueryEvent {
    // ... existing variants ...
    
    /// Emitted when an agent starts or completes
    /// Used to track when all agents are idle
    AgentStateChanged {
        agent_id: String,
        is_idle: bool, // true = agent just went idle, false = agent started
    },
}
```

- [ ] **Step 2: Add active_agents tracking to App struct**

In `src/tui/app.rs`, in the `App` struct:

```rust
pub struct App {
    // ... existing fields ...
    
    /// Track active agents for idle detection
    /// Populated on AgentStart, cleared on AgentCompleted/AgentFailed
    active_agents: HashSet<String>,
    
    /// Current main loop state
    /// None = idle, Some = active loop tracking tools
    current_main_loop: Option<MainLoopState>,
}
```

- [ ] **Step 3: Implement helper function for idle detection**

In `src/tui/app.rs`, add method:

```rust
impl App {
    /// Returns true if all agents are idle (no active agents and no tools running)
    fn all_agents_idle(&self) -> bool {
        self.active_agents.is_empty()
    }
    
    /// Called when ResponseComplete fires to check if loop should transition to Idle
    fn check_main_loop_idle(&mut self) {
        if let Some(ref mut loop_state) = self.current_main_loop {
            if self.all_agents_idle() && loop_state.tools.iter().all(|t| !matches!(t.status, ToolStatus::Running)) {
                loop_state.status = MainLoopStatus::Idle;
            }
        }
    }
}
```

- [ ] **Step 4: Update event handlers for agent state tracking**

Modify `handle_event` in `App`:

```rust
// On AgentStart or similar event
QueryEvent::AgentStart { agent_id, .. } => {
    self.active_agents.insert(agent_id);
}

// On AgentCompleted or AgentFailed
QueryEvent::AgentCompleted { agent_id, .. } => {
    self.active_agents.remove(&agent_id);
    self.check_main_loop_idle();
}
```

- [ ] **Step 5: Run cargo check and commit**

```bash
cargo check
git add src/tui/app.rs src/query/types.rs
git commit -m "feat: implement agent idle tracking for loop boundaries"
```

---

### Task 3: Create MainLoopState management in app.rs

**Files:**
- Modify: `src/tui/app.rs`

**Steps:**

- [ ] **Step 1: Add main loop management methods**

In `src/tui/app.rs`, add to `App` impl:

```rust
impl App {
    /// Create a new main loop when user sends a message
    fn start_main_loop(&mut self) {
        let loop_id = format!("loop_{}", uuid::Uuid::new_v4());
        self.current_main_loop = Some(MainLoopState {
            loop_id,
            started_at: SystemTime::now(),
            status: MainLoopStatus::Active,
            tools: Vec::new(),
        });
        self.active_agents.clear();
    }
    
    /// Add tool to current main loop
    fn add_tool_to_current_loop(&mut self, tool_state: ToolCallState) {
        if let Some(ref mut loop_state) = self.current_main_loop {
            loop_state.tools.push(tool_state);
        }
    }
    
    /// Find and update tool state in current loop
    fn update_tool_in_current_loop(&mut self, tool_use_id: &str, updater: impl Fn(&mut ToolCallState)) {
        if let Some(ref mut loop_state) = self.current_main_loop {
            if let Some(tool) = loop_state.tools.iter_mut().find(|t| t.tool_use_id == tool_use_id) {
                updater(tool);
            }
        }
    }
    
    /// Clear main loop state (on /clear command)
    fn clear_main_loops(&mut self) {
        self.current_main_loop = None;
        self.active_agents.clear();
    }
}
```

- [ ] **Step 2: Update ToolCallStart handler**

Modify event handler:

```rust
QueryEvent::ToolCallStart { tool_use_id, tool_name, parent_agent_id, parent_tool_use_id, .. } => {
    let tool_state = ToolCallState {
        tool_use_id: tool_use_id.clone(),
        parent_agent_id,
        parent_tool_use_id,
        tool_name,
        status: ToolStatus::Running,
        created_at: SystemTime::now(),
        params: None,
        result: None,
    };
    self.add_tool_to_current_loop(tool_state);
}
```

- [ ] **Step 3: Update ToolCallInputReady handler**

```rust
QueryEvent::ToolCallInputReady { tool_use_id, params, .. } => {
    self.update_tool_in_current_loop(&tool_use_id, |tool| {
        tool.params = Some(params);
    });
}
```

- [ ] **Step 4: Update ToolCallComplete handler**

```rust
QueryEvent::ToolCallComplete { tool_use_id, result, .. } => {
    let elapsed = /* Calculate from created_at */;
    self.update_tool_in_current_loop(&tool_use_id, |tool| {
        tool.status = ToolStatus::Completed(elapsed);
        tool.result = Some(ToolResult {
            content: result.clone().unwrap_or_default(),
            full_content_available: result.is_some(),
        });
    });
}
```

- [ ] **Step 5: Run cargo check and commit**

```bash
cargo check
git add src/tui/app.rs
git commit -m "feat: implement MainLoopState management in app"
```

---

### Task 4: Add Tauri IPC event emission

**Files:**
- Modify: `src/tui/app.rs` or create `src/services/tauri_bridge.rs`

**Steps:**

- [ ] **Step 1: Create helper for emitting Tauri events**

In `src/tui/app.rs` (or new `src/services/tauri_bridge.rs`):

```rust
/// Emit MainLoopState to frontend via Tauri
#[cfg(feature = "tauri")]
pub fn emit_main_loop_state_updated(state: &MainLoopState) {
    use tauri::Manager;
    if let Ok(window) = tauri::api::dialog::ask("", "") {
        // Send via Tauri event
        // window.emit("main-loop-state-updated", &state).ok();
        // (Implementation depends on Tauri app context access)
    }
}
```

- [ ] **Step 2: Call emit in update_tool_in_current_loop**

After updating tool state, emit:

```rust
if let Some(ref loop_state) = self.current_main_loop {
    emit_main_loop_state_updated(loop_state);
}
```

- [ ] **Step 3: Emit on check_main_loop_idle**

```rust
fn check_main_loop_idle(&mut self) {
    if let Some(ref mut loop_state) = self.current_main_loop {
        // ... idle check logic ...
        if should_be_idle {
            emit_main_loop_state_updated(loop_state);
        }
    }
}
```

- [ ] **Step 4: Run cargo check and commit**

```bash
cargo check
git add src/tui/app.rs
git commit -m "feat: add Tauri event emission for state updates"
```

---

## Phase 2: Event Flow & Integration

### Task 5: Update event handlers for tool state consistency

**Files:**
- Modify: `src/tui/app.rs`

**Steps:**

- [ ] **Step 1: Verify ToolStatus elapsed time calculation**

In `ToolCallComplete` handler, calculate elapsed:

```rust
let elapsed = tool.created_at
    .elapsed()
    .map(|d| d.as_millis() as u64)
    .unwrap_or(0);
tool.status = ToolStatus::Completed(elapsed);
```

- [ ] **Step 2: Handle ToolCallFailed event**

```rust
QueryEvent::ToolCallFailed { tool_use_id, error, .. } => {
    self.update_tool_in_current_loop(&tool_use_id, |tool| {
        tool.status = ToolStatus::Failed(error.clone());
    });
}
```

- [ ] **Step 3: Update ResponseComplete handler**

On `ResponseComplete`, check idle:

```rust
QueryEvent::ResponseComplete { .. } => {
    self.check_main_loop_idle();
}
```

- [ ] **Step 4: Handle /clear command**

Ensure `/clear` calls `clear_main_loops()`:

```rust
// In command handler for /clear
self.clear_main_loops();
```

- [ ] **Step 5: Test with cargo check and commit**

```bash
cargo check
git add src/tui/app.rs
git commit -m "feat: update event handlers for state consistency"
```

---

### Task 6: Add Tauri command for state retrieval

**Files:**
- Modify: `src-tauri/src/main.rs` or create `src/commands/get_main_loop_state.rs`

**Steps:**

- [ ] **Step 1: Create Tauri command handler**

```rust
// src/commands/get_main_loop_state.rs (new)
use crate::types::MainLoopState;

#[tauri::command]
pub fn get_main_loop_state(app_state: tauri::State<'_, AppState>) -> Option<MainLoopState> {
    app_state.app.lock().unwrap().current_main_loop.clone()
}
```

- [ ] **Step 2: Register command in main.rs**

```rust
// In Tauri builder:
tauri::Builder::default()
    .invoke_handler(tauri::generate_handler![
        get_main_loop_state,
        // ... existing commands ...
    ])
```

- [ ] **Step 3: Run cargo check and commit**

```bash
cargo check
git add src/commands/get_main_loop_state.rs src-tauri/src/main.rs
git commit -m "feat: add Tauri command for retrieving main loop state"
```

---

## Phase 3: Frontend UI

### Task 7: Add [Subagent: xxx] labels to Agent messages (left panel)

**Files:**
- Modify: `src/tui/ui.rs` or frontend rendering code

**Steps:**

- [ ] **Step 1: Update Agent message rendering**

When rendering `Message::Agent`, extract `source_agent_id` and display label:

```rust
// In UI rendering function for Agent messages
if let Some(agent_id) = &agent_message.source_agent_id {
    write!(f, "[Subagent: {}] {}", agent_id, agent_message.content)?;
} else {
    write!(f, "[Agent] {}", agent_message.content)?;
}
```

- [ ] **Step 2: Filter which messages appear in left panel**

Ensure only these message types are rendered:
- `User`
- `Assistant` (all)
- `Agent` (all, with label)
- `System` (relevant ones)
- `Thinking` (if expanded)

Hide:
- Grouped/collapsed tool structures

- [ ] **Step 3: Test in TUI and verify labels display**

Run app and check that agent messages show `[Subagent: planner]` style.

- [ ] **Step 4: Commit**

```bash
git add src/tui/ui.rs
git commit -m "feat: add [Subagent: xxx] labels to agent messages in left panel"
```

---

### Task 8: Implement MainLoopState UI component (right panel)

**Files:**
- Modify: `src/tui/ui.rs` (TUI right panel section)

**Steps:**

- [ ] **Step 1: Define right panel layout**

```rust
// In ui.rs layout function
let right_panel_width = total_width * 30 / 100; // 30% width
let left_panel_width = total_width * 70 / 100;  // 70% width
```

- [ ] **Step 2: Render MainLoopState hierarchy**

```rust
fn render_main_loop_state(frame: &mut Frame, state: &MainLoopState, area: Rect) {
    // Render loop header: "Main Loop #1 [Active]"
    // For each tool in state.tools:
    //   Render with indentation based on parent_agent_id depth
    //   Show: [●/✓/✗] tool_name (elapsed_ms)
    //   Sub-info: params, result (truncated)
}
```

- [ ] **Step 3: Implement status icon rendering**

```rust
fn status_icon_and_color(status: &ToolStatus) -> (&'static str, Color) {
    match status {
        ToolStatus::Running => ("●", Color::Cyan),
        ToolStatus::Completed(_) => ("✓", Color::Green),
        ToolStatus::Failed(_) => ("✗", Color::Red),
    }
}
```

- [ ] **Step 4: Implement hierarchy indentation**

Calculate depth from `parent_agent_id` chain and indent by 2 spaces per level.

- [ ] **Step 5: Test rendering and commit**

```bash
cargo check
git add src/tui/ui.rs
git commit -m "feat: implement MainLoopState UI component in right panel"
```

---

### Task 9: State-based rendering & immutable updates (Frontend)

**Files:**
- Modify: `src/tui/app.rs` (state updates)
- Modify: Frontend state listener (React/Vue if applicable)

**Steps:**

- [ ] **Step 1: Ensure all state updates are immutable**

In `app.rs`, all mutations must create new state objects:

```rust
// BAD:
loop_state.tools.push(tool); // Mutation

// GOOD:
let mut tools = loop_state.tools.clone();
tools.push(tool);
let new_loop_state = MainLoopState { tools, ..loop_state.clone() };
```

- [ ] **Step 2: Set up Tauri event listener in frontend**

```typescript
// Frontend code (React example)
useEffect(() => {
  const unlisten = listen('main-loop-state-updated', (event: { payload: MainLoopState }) => {
    setMainLoopState(event.payload); // Immutable replace
  });
  return unlisten;
}, []);
```

- [ ] **Step 3: Implement re-render on state change**

State change → automatic UI re-render (framework handles this).

- [ ] **Step 4: Commit immutable pattern**

```bash
git add src/tui/app.rs
git commit -m "refactor: ensure MainLoopState updates use immutable patterns"
```

---

## Phase 4: Testing & Polish

### Task 10: Unit tests for state transitions

**Files:**
- Modify: `src/types/tool_state.rs` (add tests)
- Create: `tests/tool_state_test.rs`

**Steps:**

- [ ] **Step 1: Write test for ToolCallState creation**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn tool_state_starts_in_running_state() {
        let tool = ToolCallState {
            tool_use_id: "tool_1".to_string(),
            parent_agent_id: None,
            parent_tool_use_id: None,
            tool_name: "Read".to_string(),
            status: ToolStatus::Running,
            created_at: SystemTime::now(),
            params: None,
            result: None,
        };
        assert!(matches!(tool.status, ToolStatus::Running));
    }
}
```

- [ ] **Step 2: Write test for state transitions**

```rust
#[test]
fn tool_transitions_from_running_to_completed() {
    let mut tool = create_test_tool();
    tool.status = ToolStatus::Completed(150);
    assert!(matches!(tool.status, ToolStatus::Completed(150)));
}

#[test]
fn tool_can_transition_to_failed() {
    let mut tool = create_test_tool();
    tool.status = ToolStatus::Failed("timeout".to_string());
    assert!(matches!(tool.status, ToolStatus::Failed(_)));
}
```

- [ ] **Step 3: Write test for MainLoopState idle detection**

```rust
#[test]
fn main_loop_is_idle_when_no_running_tools() {
    let loop_state = MainLoopState {
        loop_id: "loop_1".to_string(),
        started_at: SystemTime::now(),
        status: MainLoopStatus::Active,
        tools: vec![
            create_completed_tool(),
            create_failed_tool(),
        ],
    };
    // Should transition to Idle when check runs
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib
```

Expected: All tests pass.

- [ ] **Step 5: Commit tests**

```bash
git add src/types/tool_state.rs tests/tool_state_test.rs
git commit -m "test: add unit tests for ToolCallState and MainLoopState transitions"
```

---

### Task 11: Integration test (tool execution → UI update)

**Files:**
- Create: `tests/main_loop_integration_test.rs`

**Steps:**

- [ ] **Step 1: Write integration test for full tool lifecycle**

```rust
#[tokio::test]
async fn full_tool_lifecycle_updates_state() {
    let mut app = setup_test_app();
    
    // Simulate UserMessage event
    app.start_main_loop();
    
    // Simulate ToolCallStart
    let tool_state = create_test_tool();
    app.add_tool_to_current_loop(tool_state.clone());
    assert_eq!(app.current_main_loop.as_ref().unwrap().tools.len(), 1);
    
    // Simulate ToolCallInputReady
    app.update_tool_in_current_loop(&tool_state.tool_use_id, |tool| {
        tool.params = Some(json!({"path": "/test"}));
    });
    
    // Simulate ToolCallComplete
    app.update_tool_in_current_loop(&tool_state.tool_use_id, |tool| {
        tool.status = ToolStatus::Completed(100);
    });
    
    // Verify state updated
    let final_tool = &app.current_main_loop.as_ref().unwrap().tools[0];
    assert!(matches!(final_tool.status, ToolStatus::Completed(100)));
}
```

- [ ] **Step 2: Test idle detection**

```rust
#[test]
fn idle_detection_triggers_when_all_tools_complete() {
    let mut app = setup_test_app();
    app.start_main_loop();
    
    // Add tool
    app.add_tool_to_current_loop(create_test_tool());
    
    // Complete tool
    app.update_tool_in_current_loop("tool_1", |tool| {
        tool.status = ToolStatus::Completed(100);
    });
    
    // Mark agent idle
    app.active_agents.clear();
    app.check_main_loop_idle();
    
    assert!(matches!(
        app.current_main_loop.as_ref().unwrap().status,
        MainLoopStatus::Idle
    ));
}
```

- [ ] **Step 3: Run integration tests**

```bash
cargo test --test main_loop_integration_test
```

Expected: All tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/main_loop_integration_test.rs
git commit -m "test: add integration test for tool lifecycle and state updates"
```

---

### Task 12: Verify `/clear` resets state

**Files:**
- Modify: Command handler for `/clear`

**Steps:**

- [ ] **Step 1: Write test for clear behavior**

```rust
#[test]
fn clear_command_resets_main_loop_state() {
    let mut app = setup_test_app();
    app.start_main_loop();
    app.add_tool_to_current_loop(create_test_tool());
    
    // Verify state exists
    assert!(app.current_main_loop.is_some());
    
    // Clear
    app.clear_main_loops();
    
    // Verify state reset
    assert!(app.current_main_loop.is_none());
    assert!(app.active_agents.is_empty());
}
```

- [ ] **Step 2: Verify /clear command calls clear_main_loops()**

In command handler, ensure:

```rust
Command::Clear => {
    app.clear_main_loops();
    // ... rest of clear logic ...
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib -- clear
```

Expected: All clear-related tests pass.

- [ ] **Step 4: Manual verification**

Run app, generate some tool calls, verify right panel populates, then `/clear` and verify panel resets.

- [ ] **Step 5: Commit**

```bash
git add src/tui/app.rs tests/
git commit -m "test: verify /clear command resets main loop state"
```

---

## Final Steps

### Task 13: Integration test with Frontend (Manual)

**Steps:**

- [ ] **Step 1: Run full app**

```bash
cargo tauri dev
```

- [ ] **Step 2: Trigger agent execution**

Submit a prompt that triggers tool calls.

- [ ] **Step 3: Verify right panel**

- Tool calls appear with status icons (●)
- Elapsed time updates in real-time
- Parameters visible
- Status transitions: ● → ✓ (elapsed 1.5s)

- [ ] **Step 4: Verify left panel**

- Agent messages show `[Subagent: xxx]` labels
- Subagent responses visible
- No tool execution info (right panel only)

- [ ] **Step 5: Test hierarchy**

If subagent calls tools:
- Right panel shows indentation (main → subagent → tools)

- [ ] **Step 6: Test /clear**

Type `/clear` → right panel clears completely.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: complete split-screen UI with state-based agent communication"
```

---

## Architecture Notes

### State Flow

```
User Input
  ↓
QueryEvent::UserMessage
  ↓
App::start_main_loop() — Create MainLoopState(Active)
  ↓
QueryEvent::ToolCallStart
  ↓
App::add_tool_to_current_loop() — Create ToolCallState(Running)
  ↓
emit_main_loop_state_updated() → Tauri event
  ↓
Frontend listens, merges state immutably, re-renders right panel
  ↓
QueryEvent::ToolCallInputReady
  ↓
App::update_tool_in_current_loop(params)
  ↓
emit → Frontend re-renders (params visible)
  ↓
QueryEvent::ToolCallComplete
  ↓
App::update_tool_in_current_loop(status: Completed, result)
  ↓
emit → Frontend re-renders (✓ icon, result)
  ↓
QueryEvent::ResponseComplete
  ↓
App::check_main_loop_idle()
  ↓
IF all_agents_idle() → MainLoopStatus::Idle
  ↓
emit → Frontend shows loop as complete
```

### Key Invariants

- **Single Source of Truth**: All state in Rust `App` struct
- **Immutable Frontend**: State pushed to frontend, not pulled
- **Hierarchy via IDs**: `parent_agent_id` determines nesting level
- **Real-time Updates**: Tauri event on every state change
- **Clear Resets**: `/clear` wipes entire `MainLoopState`

---

## Testing Strategy

| Layer | Test Type | Coverage |
|-------|-----------|----------|
| **Unit** | State transitions (Running → Completed) | 80%+ |
| **Integration** | Full tool lifecycle (start → result) | Subagent + main agent scenarios |
| **Manual** | UI rendering, real-time updates, /clear | Visual verification |

---

## Commit Summary

```
1. types: define ToolCallState and MainLoopState
2. feat: implement agent idle tracking
3. feat: implement MainLoopState management in app
4. feat: add Tauri IPC event emission
5. feat: update event handlers for state consistency
6. feat: add Tauri command for state retrieval
7. feat: add [Subagent: xxx] labels to agent messages
8. feat: implement MainLoopState UI in right panel
9. refactor: ensure immutable state updates
10. test: add unit tests for state transitions
11. test: add integration tests for tool lifecycle
12. test: verify /clear resets state
13. feat: complete split-screen UI
```
