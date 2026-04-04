use async_trait::async_trait;

use crate::services::cron::CronStore;
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct CronDeleteTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "CronDelete".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for CronDeleteTool {
    fn name(&self) -> &str {
        "CronDelete"
    }

    fn description(&self) -> &str {
        "Cancel a scheduled cron job."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The ID of the cron job to delete"
                }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let id = ctx
            .input
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'id' parameter"))?;

        let mut state = ctx.app_state.write().await;
        let store = state.cron_store.get_or_insert_with(CronStore::new);
        let removed = store.remove(id).map_err(|e| tool_err(e))?;

        let result = serde_json::json!({
            "id": removed.id,
            "deleted": true,
        });

        Ok(ToolResult {
            content: result.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn should_defer(&self) -> bool {
        true
    }

    fn search_hint(&self) -> Option<&str> {
        Some("cancel a scheduled cron job")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(CronDeleteTool.name(), "CronDelete");
    }

    #[test]
    fn test_schema() {
        let schema = CronDeleteTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["id"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("id")));
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let ctx = ToolUseContext::test(serde_json::json!({
            "id": "does-not-exist"
        }));

        let result = CronDeleteTool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_id() {
        let ctx = ToolUseContext::test(serde_json::json!({}));

        let result = CronDeleteTool.execute(ctx).await;
        assert!(result.is_err());
    }
}
