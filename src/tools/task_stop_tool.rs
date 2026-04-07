use async_trait::async_trait;

use crate::state::BackgroundTaskStatus;
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct TaskStopTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "TaskStop".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for TaskStopTool {
    fn name(&self) -> &str {
        "TaskStop"
    }

    fn description(&self) -> &str {
        "Stop a running background task by its ID."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the background task to stop"
                }
            },
            "required": ["task_id"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let task_id = ctx
            .input
            .get("task_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing 'task_id' parameter"))?
            .to_string();

        let mut state = ctx.app_state.write().await;

        let task = state
            .background_tasks
            .get_mut(&task_id)
            .ok_or_else(|| tool_err(format!("Task '{}' not found", task_id)))?;

        if task.status != BackgroundTaskStatus::Running {
            return Err(tool_err(format!(
                "Task '{}' is not running (status: {:?})",
                task_id, task.status
            )));
        }

        task.cancel_token.cancel(crate::utils::abort_signal::AbortReason::UserCancel);
        task.status = BackgroundTaskStatus::Stopped;

        let task_type = task.task_type.clone();

        let result = serde_json::json!({
            "message": "Task stopped",
            "task_id": task_id,
            "task_type": task_type,
        });

        Ok(ToolResult {
            content: result.to_string(),
            is_error: false,
            content_blocks: None,
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
        assert_eq!(TaskStopTool.name(), "TaskStop");
    }

    #[test]
    fn test_schema() {
        let schema = TaskStopTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["task_id"].is_object());
        assert_eq!(schema["required"][0], "task_id");
    }

    #[tokio::test]
    async fn test_missing_param() {
        let tool = TaskStopTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
