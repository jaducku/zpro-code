use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct TaskOutputTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "TaskOutput".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for TaskOutputTool {
    fn name(&self) -> &str {
        "TaskOutput"
    }

    fn description(&self) -> &str {
        "Get the output and status of a background task by its ID."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The ID of the background task to retrieve output for"
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

        let state = ctx.app_state.read().await;

        let task = state
            .background_tasks
            .get(&task_id)
            .ok_or_else(|| tool_err(format!("Task '{}' not found", task_id)))?;

        let output = task.output.lock().unwrap().clone();
        let status = format!("{:?}", task.status);

        let content = if output.is_empty() {
            format!("No output yet\nStatus: {}", status)
        } else {
            format!("{}\nStatus: {}", output, status)
        };

        Ok(ToolResult {
            content,
            is_error: false,
        })
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
        assert_eq!(TaskOutputTool.name(), "TaskOutput");
    }

    #[test]
    fn test_schema() {
        let schema = TaskOutputTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["task_id"].is_object());
        assert_eq!(schema["required"][0], "task_id");
    }

    #[tokio::test]
    async fn test_missing_param() {
        let tool = TaskOutputTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
