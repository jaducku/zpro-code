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

### Add `app_state` to ToolUseContext

TS uses `context.getAppState()` for direct AppState access from tools. Add `app_state: Arc<RwLock<AppState>>` to `ToolUseContext` (matching TS pattern). Pass from `QueryContext` through `execute_tool()`. Update `ToolUseContext::test()`.

### Extend AppState

Add to `AppState`:
```rust
/// Background tasks (bash, agents) — runtime only, not persisted
pub background_tasks: HashMap<String, BackgroundTask>,
/// V1 legacy todo list
pub todos: Vec<TodoItem>,
```

### BackgroundTask struct

```rust
pub struct BackgroundTask {
    pub id: String,
    pub task_type: String,           // "shell", "agent"
    pub command: Option<String>,
    pub status: BackgroundTaskStatus, // Running, Completed, Failed, Stopped
    pub cancel_token: CancellationToken,
    pub output: Arc<Mutex<String>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}
```

### TodoItem struct

```rust
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub completed: bool,
}
```

## Design

### Task List Service (`src/services/task_list/mod.rs`)

File-based CRUD for task lists. Each session has its own task directory.

**Storage:** `~/.atom/tasks/{list_id}/{task_id}.json`

**Task struct:**
```rust
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,          // Pending, InProgress, Completed
    pub active_form: Option<String>,
    pub owner: Option<String>,
    pub blocks: Vec<String>,
    pub blocked_by: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}
```

**API:**
```rust
pub fn create_task(list_id: &str, subject: &str, description: &str, active_form: Option<&str>, metadata: Option<HashMap<String, serde_json::Value>>) -> Result<Task>
pub fn get_task(list_id: &str, task_id: &str) -> Result<Option<Task>>
pub fn list_tasks(list_id: &str) -> Result<Vec<Task>>
pub fn update_task(list_id: &str, task_id: &str, updates: TaskUpdate) -> Result<Task>
pub fn delete_task(list_id: &str, task_id: &str) -> Result<()>
```

**ID allocation:** Monotonically increasing numeric IDs. High water mark stored in `{list_id}/_next_id` file. Prevents ID reuse after deletes.

**Concurrency:** Simple file lock via `fs2::FileExt::lock_exclusive()` on a `{list_id}/_lock` file for writes. Reads don't lock.

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
    pub metadata: Option<HashMap<String, serde_json::Value>>,  // null values delete keys
}
```

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

**Permission:** read-only (creates a task list item, not a file the user needs to approve).

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
- `status: "deleted"` → call `task_list::delete_task()`
- Otherwise → call `task_list::update_task()` with provided fields
- Return `{ success, taskId, updatedFields }` as JSON

**Permission:** not read-only (modifies state).

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
2. Read `output.lock()` content
3. Return output text

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

Modify existing `BashTool` to handle `run_in_background: true`:
1. Check `input.run_in_background`
2. If true: generate task ID, create `BackgroundTask`, register in `app_state.background_tasks`, spawn tokio task, return immediately with `{ task_id, status: "running" }`
3. Spawned task: captures output to `Arc<Mutex<String>>`, updates status on completion

---

## Files Changed

| File | Change |
|------|--------|
| `src/services/task_list/mod.rs` | **New** — file-based task CRUD service |
| `src/services/mod.rs` | Register task_list module |
| `src/state.rs` | Add `BackgroundTask`, `BackgroundTaskStatus`, `TodoItem`, extend `AppState` |
| `src/tool.rs` | Add `app_state: Arc<RwLock<AppState>>` to ToolUseContext |
| `src/query/types.rs` | Add `app_state` to QueryContext |
| `src/query/engine.rs` | Pass `app_state` in execute_tool() |
| `src/tui/run.rs` | Pass `app_state` to QueryContext |
| `src/entrypoints/print.rs` | Pass `app_state` to QueryContext |
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
