use async_trait::async_trait;

use crate::state::TodoItem;
use crate::tool::{Tool, ToolResult, ToolUseContext};
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
        "Update the session todo list."
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
                            "id": { "type": "string" },
                            "content": { "type": "string" },
                            "completed": { "type": "boolean" }
                        },
                        "required": ["id", "content", "completed"]
                    }
                }
            },
            "required": ["todos"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let old_todos = ctx.app_state.read().await.todos.clone();

        let todos_json = ctx
            .input
            .get("todos")
            .and_then(|v| v.as_array())
            .ok_or_else(|| tool_err("Missing 'todos' parameter"))?;

        let mut new_todos: Vec<TodoItem> = Vec::with_capacity(todos_json.len());
        for (i, item) in todos_json.iter().enumerate() {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| tool_err(format!("todos[{}]: missing 'id'", i)))?
                .to_string();
            let content = item
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| tool_err(format!("todos[{}]: missing 'content'", i)))?
                .to_string();
            let completed = item
                .get("completed")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| tool_err(format!("todos[{}]: missing 'completed'", i)))?;
            new_todos.push(TodoItem { id, content, completed });
        }

        // If all todos are completed, clear the list
        if !new_todos.is_empty() && new_todos.iter().all(|t| t.completed) {
            new_todos = Vec::new();
        }

        ctx.app_state.write().await.todos = new_todos.clone();

        let result = serde_json::json!({
            "oldTodos": old_todos.iter().map(|t| serde_json::json!({
                "id": t.id,
                "content": t.content,
                "completed": t.completed,
            })).collect::<Vec<_>>(),
            "newTodos": new_todos.iter().map(|t| serde_json::json!({
                "id": t.id,
                "content": t.content,
                "completed": t.completed,
            })).collect::<Vec<_>>(),
        });

        Ok(ToolResult {
            content: result.to_string(),
            is_error: false,
        })
    }

    fn is_read_only(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(TodoWriteTool.name(), "TodoWrite");
    }

    #[test]
    fn test_schema() {
        let schema = TodoWriteTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["todos"].is_object());
        assert_eq!(schema["required"][0], "todos");
    }

    #[tokio::test]
    async fn test_missing_todos_param() {
        let tool = TodoWriteTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
