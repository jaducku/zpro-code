use async_trait::async_trait;
use std::collections::HashMap;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct TaskCreateTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "TaskCreate".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &str {
        "TaskCreate"
    }

    fn description(&self) -> &str {
        "Create a new task in the task list."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "subject": {
                    "type": "string",
                    "description": "Short subject/title of the task"
                },
                "description": {
                    "type": "string",
                    "description": "Detailed description of the task"
                },
                "activeForm": {
                    "type": "string",
                    "description": "Active form of the task (e.g. verb phrase)"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata key-value pairs"
                }
            },
            "required": ["subject", "description"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let subject = ctx
            .input
            .get("subject")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required parameter: 'subject'"))?;

        let description = ctx
            .input
            .get("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required parameter: 'description'"))?;

        let active_form = ctx.input.get("activeForm").and_then(|v| v.as_str());

        let metadata: Option<HashMap<String, serde_json::Value>> = ctx
            .input
            .get("metadata")
            .and_then(|v| v.as_object())
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect());

        let session_id = ctx.app_state.read().await.session_id.clone();
        let store = crate::services::task_list::TaskListStore::new();

        let task = store
            .create_task(&session_id, subject, description, active_form, metadata)
            .map_err(|e| tool_err(format!("Failed to create task: {}", e)))?;

        let result = serde_json::json!({
            "task_id": task.id,
            "subject": task.subject,
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
        assert_eq!(TaskCreateTool.name(), "TaskCreate");
    }

    #[test]
    fn test_schema() {
        let schema = TaskCreateTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["subject"].is_object());
        assert!(schema["properties"]["description"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("subject")));
        assert!(required.contains(&serde_json::json!("description")));
    }

    #[tokio::test]
    async fn test_missing_params() {
        let tool = TaskCreateTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
