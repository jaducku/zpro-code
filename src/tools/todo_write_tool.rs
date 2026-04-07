use async_trait::async_trait;

use crate::state::TodoItem;
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;
use crate::types::AtomError;

pub struct TodoWriteTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "TodoWrite".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "TodoWrite"
    }

    fn description(&self) -> &str {
        "Update the task list shown to the user. Use this to track progress on multi-step tasks. Send the complete list each time — this replaces the existing list."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "todos": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string", "description": "Unique identifier for the task" },
                            "content": { "type": "string", "description": "Task description" },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Task status"
                            }
                        },
                        "required": ["id", "content", "status"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let todos_value = ctx
            .input
            .get("todos")
            .ok_or_else(|| tool_err("Missing 'todos' field"))?;

        let todos: Vec<TodoItem> = serde_json::from_value(todos_value.clone())
            .map_err(|e| tool_err(format!("Invalid todos format: {}", e)))?;

        let count = todos.len();
        ctx.app_state.write().await.todos = todos;

        Ok(ToolResult {
            content: format!("{} tasks updated", count),
            is_error: false,
            content_blocks: None,
        })
    }

    fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            file_path: None,
            command: None,
            url: None,
        }
    }

    fn is_read_only(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TodoStatus;
    use serde_json::json;

    #[test]
    fn test_todo_status_deserialize() {
        let item: TodoItem = serde_json::from_value(json!({
            "id": "1",
            "content": "Test task",
            "status": "in_progress"
        }))
        .unwrap();
        assert_eq!(item.status, TodoStatus::InProgress);
    }

    #[test]
    fn test_todo_status_all_variants() {
        for (s, expected) in [
            ("pending", TodoStatus::Pending),
            ("in_progress", TodoStatus::InProgress),
            ("completed", TodoStatus::Completed),
        ] {
            let item: TodoItem = serde_json::from_value(json!({
                "id": "1",
                "content": "t",
                "status": s
            }))
            .unwrap();
            assert_eq!(item.status, expected);
        }
    }

    #[test]
    fn test_name() {
        assert_eq!(TodoWriteTool.name(), "TodoWrite");
    }

    #[test]
    fn test_schema_structure() {
        let schema = TodoWriteTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["todos"].is_object());
        assert_eq!(schema["required"][0], "todos");
        let status_enum = &schema["properties"]["todos"]["items"]["properties"]["status"]["enum"];
        assert!(status_enum.as_array().unwrap().contains(&json!("pending")));
        assert!(status_enum
            .as_array()
            .unwrap()
            .contains(&json!("in_progress")));
        assert!(status_enum
            .as_array()
            .unwrap()
            .contains(&json!("completed")));
    }

    #[tokio::test]
    async fn test_missing_todos_param() {
        let tool = TodoWriteTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_updates_state() {
        let tool = TodoWriteTool;
        let ctx = ToolUseContext::test(json!({
            "todos": [
                { "id": "1", "content": "First task", "status": "pending" },
                { "id": "2", "content": "Active task", "status": "in_progress" }
            ]
        }));
        let state = ctx.app_state.clone();
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content, "2 tasks updated");
        let todos = &state.read().await.todos;
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].status, TodoStatus::Pending);
        assert_eq!(todos[1].status, TodoStatus::InProgress);
    }
}
