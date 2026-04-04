# Batch 2: Task System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add task management (file-based task list + background task control + todo list) with 7 new tools, enabling agentic workflows that track and manage work.

**Architecture:** Three layers: (1) `task_list` service for file-based CRUD with atomic writes and file locking, (2) `BackgroundTask` in AppState for runtime process tracking, (3) 7 tool wrappers. AppState access added to ToolUseContext/QueryContext for tools that need runtime state.

**Tech Stack:** Rust, serde_json, fs2 (file locking), tokio, uuid

**Spec:** `docs/superpowers/specs/2026-04-04-batch2-task-system-design.md`

---

## File Structure

| File | Responsibility |
|------|---------------|
| `src/state/app_state.rs` | Add BackgroundTask, TodoItem, extend AppState |
| `src/state/mod.rs` | Re-export new types |
| `src/services/task_list/mod.rs` | **New** — file-based task CRUD |
| `src/services/mod.rs` | Register task_list module |
| `src/tool.rs` | Add `app_state` to ToolUseContext |
| `src/query/types.rs` | Add `app_state` to QueryContext |
| `src/query/engine.rs` | Pass `app_state` in execute_tool() |
| `src/tui/run.rs` | Create shared AppState, pass through |
| `src/entrypoints/print.rs` | Create AppState for print mode |
| `src/tools/bash_tool.rs` | Add `run_in_background` support |
| `src/tools/task_create_tool.rs` | **New** |
| `src/tools/task_get_tool.rs` | **New** |
| `src/tools/task_list_tool.rs` | **New** |
| `src/tools/task_update_tool.rs` | **New** |
| `src/tools/task_stop_tool.rs` | **New** |
| `src/tools/task_output_tool.rs` | **New** |
| `src/tools/todo_write_tool.rs` | **New** |
| `src/tools/mod.rs` | Register 7 new tools |
| `Cargo.toml` | Add `fs2` |

---

### Task 1: Add fs2 dependency + extend AppState

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/state/app_state.rs`
- Modify: `src/state/mod.rs`

- [ ] **Step 1: Add `fs2` to Cargo.toml**

Add after `htmd = "0.5"`:
```toml
fs2 = "0.4"
```

- [ ] **Step 2: Add BackgroundTask, BackgroundTaskStatus, TodoItem to `src/state/app_state.rs`**

Add before `AppState` struct:

```rust
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Background task (shell, agent) — runtime only, not serialized
pub struct BackgroundTask {
    pub id: String,
    pub task_type: String,
    pub command: Option<String>,
    pub status: BackgroundTaskStatus,
    pub cancel_token: CancellationToken,
    pub output: Arc<Mutex<String>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub completed: bool,
}
```

Add to `AppState` struct (after `foreground_task_id`):
```rust
    /// Background tasks — runtime only, not serialized
    #[serde(skip)]
    #[serde(default)]
    pub background_tasks: HashMap<String, BackgroundTask>,
    /// V1 legacy todo list
    #[serde(default)]
    pub todos: Vec<TodoItem>,
```

Add to `Default for AppState` impl:
```rust
    background_tasks: HashMap::new(),
    todos: Vec::new(),
```

Note: `BackgroundTask` doesn't derive `Clone` or `Serialize` (has `CancellationToken` and `Arc<Mutex>`). The `#[serde(skip)]` on `background_tasks` handles this.

- [ ] **Step 3: Update `src/state/mod.rs` re-exports**

```rust
pub use app_state::{AppState, BackgroundTask, BackgroundTaskStatus, ConnectionStatus, ExpandedView, TodoItem, ViewSelectionMode};
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check 2>&1 | tail -5`

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/state/app_state.rs src/state/mod.rs
git commit -m "feat: add BackgroundTask, TodoItem to AppState, add fs2 dep"
```

---

### Task 2: Add app_state to ToolUseContext and QueryContext

**Files:**
- Modify: `src/tool.rs`
- Modify: `src/query/types.rs`
- Modify: `src/query/engine.rs`
- Modify: `src/tui/run.rs`
- Modify: `src/entrypoints/print.rs`
- Modify: `src/tools/skill_tool.rs`

- [ ] **Step 1: Add `app_state` to ToolUseContext in `src/tool.rs`**

Add field:
```rust
    pub app_state: Arc<tokio::sync::RwLock<crate::state::AppState>>,
```

Update `ToolUseContext::test()`:
```rust
    app_state: Arc::new(tokio::sync::RwLock::new(crate::state::AppState::default())),
```

- [ ] **Step 2: Add `app_state` to QueryContext in `src/query/types.rs`**

```rust
    pub app_state: Arc<tokio::sync::RwLock<crate::state::AppState>>,
```

- [ ] **Step 3: Pass `app_state` in `execute_tool()` in `src/query/engine.rs`**

Add to the ToolUseContext construction:
```rust
    app_state: ctx.app_state.clone(),
```

Update `test_query_context()`:
```rust
    app_state: Arc::new(tokio::sync::RwLock::new(crate::state::AppState::default())),
```

- [ ] **Step 4: Create shared AppState in `src/tui/run.rs`**

In `run_app()`, create early (before the event loop):
```rust
let app_state = Arc::new(tokio::sync::RwLock::new(crate::state::AppState::default()));
```

Pass to both `submit_query()` and `submit_query_skill()` QueryContext constructions:
```rust
    app_state: app_state.clone(),
```

These functions need `app_state` as a parameter. Add it to their signatures.

- [ ] **Step 5: Update `src/entrypoints/print.rs`**

```rust
    app_state: Arc::new(tokio::sync::RwLock::new(crate::state::AppState::default())),
```

- [ ] **Step 6: Update `src/tools/skill_tool.rs`**

In `execute_forked_skill()`, add to QueryContext:
```rust
    app_state: parent_ctx.app_state.clone(),
```

- [ ] **Step 7: Verify**

Run: `cargo check 2>&1 | tail -5`
Expected: Clean.

- [ ] **Step 8: Commit**

```bash
git add src/tool.rs src/query/types.rs src/query/engine.rs src/tui/run.rs src/entrypoints/print.rs src/tools/skill_tool.rs
git commit -m "feat: add app_state to ToolUseContext and QueryContext"
```

---

### Task 3: Implement task_list service

**Files:**
- Create: `src/services/task_list/mod.rs`
- Modify: `src/services/mod.rs`

- [ ] **Step 1: Create `src/services/task_list/mod.rs`**

Complete file-based CRUD service. Key implementation details:

- Storage path: `~/.atom/tasks/{list_id}/{task_id}.json`
- ID allocation: monotonic counter in `{list_id}/_next_id`
- Locking: `fs2::FileExt::lock_exclusive()` on `{list_id}/_lock`
- Atomic writes: write to `.tmp`, then `std::fs::rename()`
- Lock held during entire create (read _next_id → increment → write _next_id → write task file)

The service should expose:
```rust
pub struct Task { id, subject, description, status, active_form, owner, blocks, blocked_by, metadata }
pub enum TaskStatus { Pending, InProgress, Completed }
pub struct TaskUpdate { optional fields for partial update }

pub fn tasks_dir(list_id: &str) -> PathBuf
pub fn create_task(list_id, subject, description, active_form, metadata) -> Result<Task>
pub fn get_task(list_id, task_id) -> Result<Option<Task>>
pub fn list_tasks(list_id) -> Result<Vec<Task>>
pub fn update_task(list_id, task_id, updates) -> Result<Task>
pub fn delete_task(list_id, task_id) -> Result<()>
```

Tests:
- `test_create_and_get` — create a task, get it back
- `test_list_tasks` — create 2, list returns both
- `test_update_task` — update subject and status
- `test_delete_task` — create, delete, get returns None
- `test_id_monotonic` — create 3, IDs are 1, 2, 3
- `test_create_in_nonexistent_dir` — auto-creates directory

Use `tempfile::TempDir` for test isolation. Override the tasks_dir to use temp.

- [ ] **Step 2: Register in `src/services/mod.rs`**

Add: `pub mod task_list;`

- [ ] **Step 3: Run tests**

Run: `cargo test task_list 2>&1 | tail -10`
Expected: All pass.

- [ ] **Step 4: Commit**

```bash
git add src/services/task_list/mod.rs src/services/mod.rs
git commit -m "feat: implement file-based task_list service"
```

---

### Task 4: Implement TaskCreate, TaskGet, TaskList, TaskUpdate tools

**Files:**
- Create: `src/tools/task_create_tool.rs`
- Create: `src/tools/task_get_tool.rs`
- Create: `src/tools/task_list_tool.rs`
- Create: `src/tools/task_update_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create all 4 tool files**

Each is a thin wrapper around `task_list` service. They share a pattern:
- Extract params from `ctx.input`
- Get session_id from `ctx.app_state.read().session_id` for use as `list_id`
- Call service function
- Return result as JSON `ToolResult`

**TaskCreateTool:** `is_read_only: false`. Calls `create_task()`. Returns `{ task_id, subject }`.

**TaskGetTool:** `is_read_only: true`. Calls `get_task()`. Returns full task JSON or "not found" error.

**TaskListTool:** `is_read_only: true`. No params. Calls `list_tasks()`. Filters `metadata._internal`. Cleans completed from `blocked_by`. Returns array.

**TaskUpdateTool:** `is_read_only: false`. Handles `status: "deleted"` → `delete_task()`. Otherwise → `update_task()`. Returns `{ success, taskId, updatedFields }`.

Each tool needs basic tests: name, schema, missing params.

- [ ] **Step 2: Register all 4 in `src/tools/mod.rs`**

- [ ] **Step 3: Run tests**

Run: `cargo test task_create_tool task_get_tool task_list_tool task_update_tool 2>&1 | tail -10`

- [ ] **Step 4: Commit**

```bash
git add src/tools/task_create_tool.rs src/tools/task_get_tool.rs src/tools/task_list_tool.rs src/tools/task_update_tool.rs src/tools/mod.rs
git commit -m "feat: implement TaskCreate/Get/List/Update tools"
```

---

### Task 5: Extend BashTool with run_in_background

**Files:**
- Modify: `src/tools/bash_tool.rs`

- [ ] **Step 1: Add background execution to BashTool**

In `execute()`, before running the command, check:
```rust
let run_in_background = ctx.input.get("run_in_background")
    .and_then(|v| v.as_bool())
    .unwrap_or(false);
```

If true:
1. Generate ID: `format!("bg-{}", uuid::Uuid::new_v4())`
2. Create `BackgroundTask` with `Running` status
3. Register in `ctx.app_state.write().await.background_tasks`
4. Clone necessary values (command, output buffer, cancel_token, app_state)
5. `tokio::spawn` the async block that:
   - Runs the command
   - Appends stdout+stderr to `output.lock()`
   - Updates status in `app_state.write().await.background_tasks` to `Completed`/`Failed`
   - Checks cancel_token periodically
6. Return immediately: `{ task_id, status: "running", command }`

Note: The `run_in_background` param is already in the BashTool schema. Check if it exists — if not, add it.

Tests:
- `test_background_returns_immediately` — verify returns with task_id
- `test_background_task_registered` — check app_state has the task

- [ ] **Step 2: Run tests**

Run: `cargo test bash_tool 2>&1 | tail -10`

- [ ] **Step 3: Commit**

```bash
git add src/tools/bash_tool.rs
git commit -m "feat: add run_in_background support to BashTool"
```

---

### Task 6: Implement TaskStop and TaskOutput tools

**Files:**
- Create: `src/tools/task_stop_tool.rs`
- Create: `src/tools/task_output_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/task_stop_tool.rs`**

- Name: "TaskStop"
- Schema: `task_id` (required string)
- Execute:
  1. `ctx.app_state.write().await` to get mutable access
  2. Find task by ID in `background_tasks`
  3. Check status is `Running` (else error)
  4. Call `cancel_token.cancel()`
  5. Set status to `Stopped`
  6. Return `{ message, task_id, task_type }`
- `is_read_only: false`

- [ ] **Step 2: Create `src/tools/task_output_tool.rs`**

- Name: "TaskOutput"
- Schema: `task_id` (required string)
- Execute:
  1. `ctx.app_state.read().await`
  2. Find task in `background_tasks`
  3. `output.lock().unwrap().clone()`
  4. Return output text (or "No output yet" if empty)
- `is_read_only: true`

- [ ] **Step 3: Register in `src/tools/mod.rs`**

- [ ] **Step 4: Run tests and commit**

```bash
git add src/tools/task_stop_tool.rs src/tools/task_output_tool.rs src/tools/mod.rs
git commit -m "feat: implement TaskStop and TaskOutput tools"
```

---

### Task 7: Implement TodoWriteTool

**Files:**
- Create: `src/tools/todo_write_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create `src/tools/todo_write_tool.rs`**

- Name: "TodoWrite"
- Schema: `todos` (required array of `{ id, content, completed }`)
- Execute:
  1. Read current `ctx.app_state.read().await.todos.clone()`
  2. Parse new todos from input
  3. If all completed, set to empty vec
  4. `ctx.app_state.write().await.todos = new_todos`
  5. Return `{ oldTodos, newTodos }` as JSON
- `is_read_only: false`

Tests: name, schema, basic write

- [ ] **Step 2: Register in `src/tools/mod.rs`**

- [ ] **Step 3: Run tests and commit**

```bash
git add src/tools/todo_write_tool.rs src/tools/mod.rs
git commit -m "feat: implement TodoWriteTool"
```

---

### Task 8: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1 | tail -20`
Expected: All new tests pass. Tool count now 20 (was 13 + 7 new).

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | grep -E "task_|todo_write|bash_tool" | head -20`
Expected: No warnings in our files.

- [ ] **Step 3: Verify tool registration**

Update test assertion in `src/tools/mod.rs` to match 20 tools.

- [ ] **Step 4: Commit any fixes**
