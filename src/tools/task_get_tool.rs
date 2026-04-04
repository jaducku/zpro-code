use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct TaskGetTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "TaskGet".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for TaskGetTool {
    fn name(&self) -> &str {
        "TaskGet"
    }

    fn description(&self) -> &str {
        "Get a task by ID from the task list."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "taskId": {
                    "type": "string",
                    "description": "The ID of the task to retrieve"
                }
            },
            "required": ["taskId"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let task_id = ctx
            .input
            .get("taskId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required parameter: 'taskId'"))?;

        let session_id = ctx.app_state.read().await.session_id.clone();
        let store = crate::services::task_list::TaskListStore::new();

        let task = store
            .get_task(&session_id, task_id)
            .map_err(|e| tool_err(format!("Failed to get task: {}", e)))?;

        match task {
            Some(t) => {
                let json = serde_json::to_string(&t)
                    .map_err(|e| tool_err(format!("Failed to serialize task: {}", e)))?;
                Ok(ToolResult {
                    content: json,
                    is_error: false,
                    content_blocks: None,
                })
            }
            None => Ok(ToolResult {
                content: format!("Task not found: {}", task_id),
                is_error: true,
                content_blocks: None,
            }),
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(TaskGetTool.name(), "TaskGet");
    }

    #[test]
    fn test_schema() {
        let schema = TaskGetTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["taskId"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("taskId")));
    }
}
