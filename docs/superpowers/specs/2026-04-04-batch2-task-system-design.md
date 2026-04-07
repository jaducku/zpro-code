# Batch 2: Task System Tools Design

## Problem

atom-code-rs has no task management. The LLM cannot create/track tasks, stop background processes, or manage todo lists. This blocks agentic workflows that depend on task tracking.

## Scope

7 tools + 1 service + infrastructure changes:

1. **TaskCreateTool** — create task in file-based task list
2. **TaskGetTool** — get task by ID
3. **TaskListTool** — list all tasks
4. **TaskUpdateTool** — update task fields (status, subject, owner, dependencies)
5. **TaskStopTool** — stop a running background task
6. **TaskOutputTool** — get output from a background task
7. **TodoWriteTool** — V1 legacy todo list (AppState in-memory)

## Prerequisites

### Add `app_state` to ToolUseContext and QueryContext

TS uses `context.getAppState()` for direct AppState access from tools. Add `app_state: Arc<RwLock<AppState>>` to both `ToolUseContext` (in `src/tool.rs`) and `QueryContext` (in `src/query/types.rs`).

**Plumbing path:**
1. `run_app()` in `src/tui/run.rs` creates a single `Arc<RwLock<AppState>>` (shared for the session)
2. `submit_query()` passes it into `QueryContext.app_state`
3. `execute_tool()` in `src/query/engine.rs` passes it into `ToolUseContext.app_state`
4. `src/entrypoints/print.rs` creates its own `Arc<RwLock<AppState::default()>>` for print mode
5. `ToolUseContext::test()` uses `Arc::new(RwLock::new(AppState::default()))`

### Extend AppState (`src/state/app_state.rs`)

Note: AppState lives at `src/state/app_state.rs`, re-exported via `src/state/mod.rs`.

Add to `AppState`:
```rust
/// Background tasks (bash, agents) — runtime only, not serialized
#[serde(skip)]
#[serde(default)]
pub background_tasks: HashMap<String, BackgroundTask>,
/// V1 legacy todo list
#[serde(default)]
pub todos: Vec<TodoItem>,
```

`background_tasks` is `#[serde(skip)]` because `BackgroundTask` contains `CancellationToken` and `Arc<Mutex<String>>` which don't implement Serialize/Deserialize. These are runtime-only and not persisted.

### BackgroundTask struct (in `src/state/app_state.rs`)

```rust
/// Not inside AppState serialization — runtime only
pub struct BackgroundTask {
    pub id: String,
    pub task_type: String,           // "shell", "agent"
    pub command: Option<String>,
    pub status: BackgroundTaskStatus,
    pub cancel_token: CancellationToken,
    pub output: Arc<std::sync::Mutex<String>>,  // std::sync::Mutex — short locks only, no .await while held
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}
```

Using `std::sync::Mutex` (not tokio) for `output` because locks are short (append a line, read buffer) and never held across `.await`.

### TodoItem struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub completed: bool,
}
```

### Background task ID format

UUID v4 string (e.g., `"bg-a1b2c3d4"`). Generated via `format!("bg-{}", uuid::Uuid::new_v4())`.

## Design

### Task List Service (`src/services/task_list/mod.rs`)

File-based CRUD for task lists. Each session has its own task directory.

**Storage:** `~/.atom/tasks/{list_id}/{task_id}.json`

**Task struct:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    pub active_form: Option<String>,
    pub owner: Option<String>,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}
```

Note: `"deleted"` is NOT a `TaskStatus` variant. It's a virtual value handled only at the `TaskUpdateTool` layer — maps to `delete_task()`.

**API:**
```rust
pub fn create_task(list_id: &str, subject: &str, description: &str, active_form: Option<&str>, metadata: Option<HashMap<String, serde_json::Value>>) -> Result<Task>
pub fn get_task(list_id: &str, task_id: &str) -> Result<Option<Task>>
pub fn list_tasks(list_id: &str) -> Result<Vec<Task>>
pub fn update_task(list_id: &str, task_id: &str, updates: TaskUpdate) -> Result<Task>
pub fn delete_task(list_id: &str, task_id: &str) -> Result<()>
```

**ID allocation:** Monotonically increasing numeric IDs. High water mark stored in `{list_id}/_next_id` file. Lock MUST be held during the entire read-increment-write sequence to prevent races.

**Concurrency:** File lock via `fs2::FileExt::lock_exclusive()` on `{list_id}/_lock` file. Lock held for ALL writes (including ID allocation). Writes use **write-then-rename** pattern for atomicity: write to `{task_id}.json.tmp`, then `std::fs::rename()` to `{task_id}.json`. This ensures reads never see partial files.

**TaskUpdate struct:**
```rust
pub struct TaskUpdate {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub active_form: Option<String>,
    pub status: Option<TaskStatus>,
    pub owner: Option<String>,
    pub add_blocks: Option<Vec<String>>,
    pub add_blocked_by: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}
```

**Session resume:** Task files persist on disk. Resumed sessions (via `--continue` or `--resume`) use the same session ID as list_id, so tasks carry over.

---

### 1. TaskCreateTool

**Tool name:** `TaskCreate`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "subject": { "type": "string" },
    "description": { "type": "string" },
    "activeForm": { "type": "string" },
    "metadata": { "type": "object" }
  },
  "required": ["subject", "description"]
}
```

**Execute:** Call `task_list::create_task()` with session ID as list_id. Return `{ task_id, subject }` as JSON.

**Permission:** not read-only (writes files to disk).

---

### 2. TaskGetTool

**Tool name:** `TaskGet`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "taskId": { "type": "string" }
  },
  "required": ["taskId"]
}
```

**Execute:** Call `task_list::get_task()`. Return full task as JSON, or error if not found.

**Permission:** read-only.

---

### 3. TaskListTool

**Tool name:** `TaskList`

**Schema:**
```json
{ "type": "object", "properties": {} }
```

**Execute:** Call `task_list::list_tasks()`. Filter out tasks with `metadata._internal`. Remove completed task IDs from `blocked_by` arrays. Return task summaries as JSON.

**Permission:** read-only.

---

### 4. TaskUpdateTool

**Tool name:** `TaskUpdate`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "taskId": { "type": "string" },
    "subject": { "type": "string" },
    "description": { "type": "string" },
    "activeForm": { "type": "string" },
    "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "deleted"] },
    "owner": { "type": "string" },
    "addBlocks": { "type": "array", "items": { "type": "string" } },
    "addBlockedBy": { "type": "array", "items": { "type": "string" } },
    "metadata": { "type": "object" }
  },
  "required": ["taskId"]
}
```

**Execute:**
- `status: "deleted"` → call `task_list::delete_task()` (virtual status, not a `TaskStatus` variant)
- Otherwise → call `task_list::update_task()` with provided fields
- Return `{ success, taskId, updatedFields }` as JSON

**Permission:** not read-only.

---

### 5. TaskStopTool

**Tool name:** `TaskStop`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "task_id": { "type": "string" }
  },
  "required": ["task_id"]
}
```

**Execute:**
1. Read `app_state.background_tasks` to find task by ID
2. Validate status is `Running`
3. Call `cancel_token.cancel()`
4. Set status to `Stopped`
5. Return `{ message, task_id, task_type }`

**Error:** Task not found, task not running.

**Permission:** not read-only.

---

### 6. TaskOutputTool

**Tool name:** `TaskOutput`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "task_id": { "type": "string" }
  },
  "required": ["task_id"]
}
```

**Execute:**
1. Read `app_state.background_tasks` to find task by ID
2. Read `output.lock()` content (std::sync::Mutex, short lock)
3. Return output text

**Note:** Output is available only after the background command completes (full buffer, not streaming). Streaming partial output is deferred.

**Error:** Task not found.

**Permission:** read-only.

---

### 7. TodoWriteTool

**Tool name:** `TodoWrite`

**Schema:**
```json
{
  "type": "object",
  "properties": {
    "todos": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "content": { "type": "string" },
          "completed": { "type": "boolean" }
        },
        "required": ["id", "content", "completed"]
      }
    }
  },
  "required": ["todos"]
}
```

**Execute:**
1. Read current `app_state.todos`
2. Replace with new list
3. If all completed, clear the list
4. Return `{ oldTodos, newTodos }`

**Permission:** not read-only.

---

### BashTool Background Extension

Modify existing `BashTool` (`src/tools/bash_tool.rs`) to handle `run_in_background: true`:

1. Check `input.get("run_in_background")` (already in schema as optional boolean)
2. If true:
   a. Generate task ID: `format!("bg-{}", uuid::Uuid::new_v4())`
   b. Create `BackgroundTask` with `status: Running`, `cancel_token`, `output: Arc<Mutex<String>>`
   c. Register in `app_state.write().background_tasks`
   d. Spawn tokio task: run command, append stdout+stderr to `output` buffer, set status on completion
   e. Return immediately: `{ task_id, status: "running", command }`
3. Spawned task: uses `Command::new("bash").arg("-c").arg(command).output().await`, writes full result to buffer (not streaming — V1). Checks cancel_token for early termination.

**Note:** The existing `AppState.tasks` field (`HashMap<String, serde_json::Value>`) is for coordinator/internal tasks and is separate from `background_tasks`. They coexist without conflict.

---

## Files Changed

| File | Change |
|------|--------|
| `src/services/task_list/mod.rs` | **New** — file-based task CRUD service |
| `src/services/mod.rs` | Register task_list module |
| `src/state/app_state.rs` | Add `BackgroundTask`, `BackgroundTaskStatus`, `TodoItem`, extend `AppState` |
| `src/tool.rs` | Add `app_state: Arc<RwLock<AppState>>` to ToolUseContext |
| `src/query/types.rs` | Add `app_state` to QueryContext |
| `src/query/engine.rs` | Pass `app_state` in execute_tool() |
| `src/tui/run.rs` | Create shared `Arc<RwLock<AppState>>`, pass to QueryContext |
| `src/entrypoints/print.rs` | Create `Arc<RwLock<AppState::default()>>`, pass to QueryContext |
| `src/tools/bash_tool.rs` | Add `run_in_background` handling |
| `src/tools/task_create_tool.rs` | **New** |
| `src/tools/task_get_tool.rs` | **New** |
| `src/tools/task_list_tool.rs` | **New** |
| `src/tools/task_update_tool.rs` | **New** |
| `src/tools/task_stop_tool.rs` | **New** |
| `src/tools/task_output_tool.rs` | **New** |
| `src/tools/todo_write_tool.rs` | **New** |
| `src/tools/mod.rs` | Register 7 new tools |
| `Cargo.toml` | Add `fs2` for file locking |

## Non-goals

- Task hooks (TaskCreated, TaskCompleted) — add later
- Team/swarm awareness — add with Batch 7
- Task view auto-expand in TUI — cosmetic, add later
- AgentTool background task spawning — only BashTool for now
- Streaming partial output from background tasks — V1 uses full output
- Task file cleanup/TTL — add later
