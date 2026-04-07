# TodoWrite Tool + Task Checklist Design

LLM-callable tool to manage a visible task list, displayed as a checklist in the TUI status area.

## Problem

- No way for the LLM to show task progress to the user
- Original Claude Code shows ■/□/✓ task checklist during multi-step work
- User has no visibility into what the LLM plans to do

## Reference UI

```
✦ Exploring TUI context… (59s · ↓ 918 tokens)
  ├ ■ Explore project context for reasoning/activity display
  ├ □ Ask clarifying questions
  ├ □ Propose approaches with trade-offs
  ├ □ Present design and get approval
  └ □ Write spec, review, and transition
```

---

## Section 1: Data Model

### TodoItem and TodoStatus (state.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}
```

Add to AppState:
```rust
pub todos: Vec<TodoItem>,
```

---

## Section 2: TodoWrite Tool

### Tool schema

```json
{
  "name": "TodoWrite",
  "description": "Update the task list shown to the user. Use this to track progress on multi-step tasks. Send the complete list each time — this replaces the existing list.",
  "input_schema": {
    "type": "object",
    "properties": {
      "todos": {
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "id": { "type": "string" },
            "content": { "type": "string" },
            "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
          },
          "required": ["id", "content", "status"]
        }
      }
    },
    "required": ["todos"]
  }
}
```

### Implementation (tools/todo_write_tool.rs)

```rust
async fn execute(&self, ctx: ToolUseContext) -> Result<ToolResult> {
    let todos: Vec<TodoItem> = parse from ctx.input["todos"];
    let mut state = ctx.app_state.write().await;
    state.todos = todos;
    Ok(ToolResult { content: "Todos updated".into(), is_error: false, content_blocks: None })
}
```

### Tool registration

Add to tool list in `src/tools/mod.rs` alongside existing tools.

---

## Section 3: TUI Rendering

### Location

Below the activity/status bar, above the input area. Only shown when `todos` is non-empty.

### Layout change (ui.rs)

Current layout:
```
[Messages area]
[Activity bar - 1 line]
[Input area]
[Status bar]
```

New layout:
```
[Messages area]
[Activity bar - 1 line]
[Todo list - 0 to N lines]
[Input area]
[Status bar]
```

### Rendering

```rust
fn render_todos(app: &App, area: Rect, buf: &mut Buffer) {
    let todos = &app_state.todos;  // read from app_state
    if todos.is_empty() { return; }
    
    for (i, todo) in todos.iter().take(10).enumerate() {
        let prefix = if i == todos.len().min(10) - 1 { "└" } else { "├" };
        let (icon, color) = match todo.status {
            TodoStatus::InProgress => ("■", Color::Yellow),
            TodoStatus::Pending => ("□", Color::DarkGray),
            TodoStatus::Completed => ("✓", Color::Green),
        };
        // Render: "  {prefix} {icon} {content}"
    }
    if todos.len() > 10 {
        // Render: "  └ +{N} more tasks"
    }
}
```

---

## Section 4: File Changes

| File | Changes |
|------|---------|
| `src/state.rs` | TodoItem, TodoStatus structs, add `todos` to AppState |
| `src/tools/todo_write_tool.rs` | NEW: TodoWrite tool implementation |
| `src/tools/mod.rs` | Register TodoWriteTool |
| `src/tui/ui.rs` | render_todos function, layout adjustment |
