use async_trait::async_trait;
use std::collections::HashMap;

use crate::services::task_list::{TaskListStore, TaskStatus, TaskUpdate};
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct TaskUpdateTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "TaskUpdate".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &str {
        "TaskUpdate"
    }

    fn description(&self) -> &str {
        "Update or delete a task in the task list."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "taskId": {
                    "type": "string",
                    "description": "The ID of the task to update"
                },
                "subject": {
                    "type": "string",
                    "description": "New subject/title"
                },
                "description": {
                    "type": "string",
                    "description": "New description"
                },
                "activeForm": {
                    "type": "string",
                    "description": "New active form"
                },
                "status": {
                    "type": "string",
                    "enum": ["pending", "in_progress", "completed", "deleted"],
                    "description": "New task status. Use 'deleted' to remove the task."
                },
                "owner": {
                    "type": "string",
                    "description": "New owner"
                },
                "addBlocks": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Task IDs that this task blocks"
                },
                "addBlockedBy": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Task IDs that block this task"
                },
                "metadata": {
                    "type": "object",
                    "description": "Metadata key-value pairs to merge (null value removes key)"
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
            .ok_or_else(|| tool_err("Missing required parameter: 'taskId'"))?
            .to_string();

        let session_id = ctx.app_state.read().await.session_id.clone();
        let store = TaskListStore::new();

        // If status is "deleted", delete the task
        if let Some("deleted") = ctx.input.get("status").and_then(|v| v.as_str()) {
            store
                .delete_task(&session_id, &task_id)
                .map_err(|e| tool_err(format!("Failed to delete task: {}", e)))?;

            let result = serde_json::json!({
                "success": true,
                "taskId": task_id,
                "updatedFields": ["status"],
            });
            return Ok(ToolResult {
                content: result.to_string(),
                is_error: false,
                content_blocks: None,
            });
        }

        // Build TaskUpdate from provided fields
        let mut updated_fields = Vec::new();

        let subject = ctx.input.get("subject").and_then(|v| v.as_str()).map(|s| {
            updated_fields.push("subject");
            s.to_string()
        });

        let description = ctx
            .input
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| {
                updated_fields.push("description");
                s.to_string()
            });

        let active_form = ctx
            .input
            .get("activeForm")
            .and_then(|v| v.as_str())
            .map(|s| {
                updated_fields.push("activeForm");
                s.to_string()
            });

        let status = ctx
            .input
            .get("status")
            .and_then(|v| v.as_str())
            .map(|s| {
                updated_fields.push("status");
                match s {
                    "in_progress" => TaskStatus::InProgress,
                    "completed" => TaskStatus::Completed,
                    _ => TaskStatus::Pending,
                }
            });

        let owner = ctx.input.get("owner").and_then(|v| v.as_str()).map(|s| {
            updated_fields.push("owner");
            s.to_string()
        });

        let add_blocks: Option<Vec<String>> = ctx
            .input
            .get("addBlocks")
            .and_then(|v| v.as_array())
            .map(|arr| {
                updated_fields.push("addBlocks");
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let add_blocked_by: Option<Vec<String>> = ctx
            .input
            .get("addBlockedBy")
            .and_then(|v| v.as_array())
            .map(|arr| {
                updated_fields.push("addBlockedBy");
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });

        let metadata: Option<HashMap<String, serde_json::Value>> = ctx
            .input
            .get("metadata")
            .and_then(|v| v.as_object())
            .map(|obj| {
                updated_fields.push("metadata");
                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            });

        let updates = TaskUpdate {
            subject,
            description,
            active_form,
            status,
            owner,
            add_blocks,
            add_blocked_by,
            metadata,
        };

        store
            .update_task(&session_id, &task_id, updates)
            .map_err(|e| tool_err(format!("Failed to update task: {}", e)))?;

        let result = serde_json::json!({
            "success": true,
            "taskId": task_id,
            "updatedFields": updated_fields,
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
        assert_eq!(TaskUpdateTool.name(), "TaskUpdate");
    }

    #[test]
    fn test_schema() {
        let schema = TaskUpdateTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["taskId"].is_object());
        assert!(schema["properties"]["status"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("taskId")));
    }

    #[tokio::test]
    async fn test_missing_task_id() {
        let tool = TaskUpdateTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
