use async_trait::async_trait;

use crate::services::task_list::TaskStatus;
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct TaskListTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "TaskList".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &str {
        "TaskList"
    }

    fn description(&self) -> &str {
        "List all tasks in the task list."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let session_id = ctx.app_state.read().await.session_id.clone();
        let store = crate::services::task_list::TaskListStore::new();

        let tasks = store
            .list_tasks(&session_id)
            .map_err(|e| tool_err(format!("Failed to list tasks: {}", e)))?;

        // Collect IDs of completed tasks
        let completed_ids: std::collections::HashSet<&str> = tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Completed)
            .map(|t| t.id.as_str())
            .collect();

        // Filter out internal tasks and clean up blocked_by
        let visible: Vec<serde_json::Value> = tasks
            .iter()
            .filter(|t| !t.metadata.contains_key("_internal"))
            .map(|t| {
                let mut val = serde_json::to_value(t).unwrap_or_default();
                // Remove completed task IDs from blocked_by
                if let Some(blocked_by) = val.get_mut("blocked_by") {
                    if let Some(arr) = blocked_by.as_array_mut() {
                        arr.retain(|id| {
                            id.as_str()
                                .map(|s| !completed_ids.contains(s))
                                .unwrap_or(true)
                        });
                    }
                }
                val
            })
            .collect();

        let json = serde_json::to_string(&visible)
            .map_err(|e| tool_err(format!("Failed to serialize tasks: {}", e)))?;

        Ok(ToolResult {
            content: json,
            is_error: false,
            content_blocks: None,
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
        assert_eq!(TaskListTool.name(), "TaskList");
    }

    #[test]
    fn test_schema() {
        let schema = TaskListTool.input_schema();
        assert_eq!(schema["type"], "object");
    }
}
