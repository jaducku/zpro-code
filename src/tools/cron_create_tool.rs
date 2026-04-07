use async_trait::async_trait;

use crate::services::cron::{self, CronStore, CronTask};
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct CronCreateTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "CronCreate".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for CronCreateTool {
    fn name(&self) -> &str {
        "CronCreate"
    }

    fn description(&self) -> &str {
        "Schedule a prompt to run on a cron schedule."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "cron": {
                    "type": "string",
                    "description": "Cron expression (5-field or 6-field)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to execute on schedule"
                },
                "recurring": {
                    "type": "boolean",
                    "description": "Whether the task repeats (default true)"
                },
                "durable": {
                    "type": "boolean",
                    "description": "Whether the task persists across sessions (default false)"
                }
            },
            "required": ["cron", "prompt"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let cron_expr = ctx
            .input
            .get("cron")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'cron' parameter"))?;

        let prompt = ctx
            .input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'prompt' parameter"))?;

        let recurring = ctx
            .input
            .get("recurring")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let durable = ctx
            .input
            .get("durable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Validate the cron expression
        let normalized = cron::validate_cron(cron_expr).map_err(|e| tool_err(e))?;

        let id = uuid::Uuid::new_v4().to_string();

        let task = CronTask {
            id: id.clone(),
            cron: normalized.clone(),
            prompt: prompt.to_string(),
            recurring,
            durable,
            created_at: chrono::Utc::now(),
            last_fired_at: None,
            agent_id: None,
        };

        // Add to store (get_or_insert_with pattern)
        {
            let mut state = ctx.app_state.write().await;
            let store = state.cron_store.get_or_insert_with(CronStore::new);
            store.add(task).map_err(|e| tool_err(e))?;
        }

        let human = cron::human_schedule(cron_expr);

        let result = serde_json::json!({
            "id": id,
            "humanSchedule": human,
            "recurring": recurring,
            "durable": durable,
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

    fn should_defer(&self) -> bool {
        true
    }

    fn search_hint(&self) -> Option<&str> {
        Some("schedule a prompt to run on a cron schedule")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(CronCreateTool.name(), "CronCreate");
    }

    #[test]
    fn test_schema() {
        let schema = CronCreateTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["cron"].is_object());
        assert!(schema["properties"]["prompt"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("cron")));
        assert!(required.contains(&serde_json::json!("prompt")));
    }

    #[test]
    fn test_should_defer() {
        assert!(CronCreateTool.should_defer());
    }

    #[tokio::test]
    async fn test_create_cron() {
        let ctx = ToolUseContext::test(serde_json::json!({
            "cron": "*/5 * * * *",
            "prompt": "check status"
        }));

        let result = CronCreateTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert!(parsed["id"].as_str().is_some());
        assert!(parsed["humanSchedule"].as_str().unwrap().contains("5 minutes"));
        assert_eq!(parsed["recurring"], true);
        assert_eq!(parsed["durable"], false);
    }

    #[tokio::test]
    async fn test_invalid_cron() {
        let ctx = ToolUseContext::test(serde_json::json!({
            "cron": "not valid",
            "prompt": "test"
        }));

        let result = CronCreateTool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_cron() {
        let ctx = ToolUseContext::test(serde_json::json!({
            "prompt": "test"
        }));

        let result = CronCreateTool.execute(ctx).await;
        assert!(result.is_err());
    }
}
