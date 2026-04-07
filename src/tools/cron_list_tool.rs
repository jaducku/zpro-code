use async_trait::async_trait;

use crate::services::cron;
use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct CronListTool;

#[async_trait]
impl Tool for CronListTool {
    fn name(&self) -> &str {
        "CronList"
    }

    fn description(&self) -> &str {
        "List all scheduled cron jobs."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let state = ctx.app_state.read().await;
        let tasks = match &state.cron_store {
            Some(store) => store.list(None),
            None => Vec::new(),
        };

        let jobs: Vec<serde_json::Value> = tasks
            .iter()
            .map(|t| {
                serde_json::json!({
                    "id": t.id,
                    "cron": t.cron,
                    "humanSchedule": cron::human_schedule(&t.cron),
                    "prompt": t.prompt,
                    "recurring": t.recurring,
                    "durable": t.durable,
                })
            })
            .collect();

        let result = serde_json::json!({ "jobs": jobs });

        Ok(ToolResult {
            content: result.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    fn should_defer(&self) -> bool {
        true
    }

    fn search_hint(&self) -> Option<&str> {
        Some("list all scheduled cron jobs")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(CronListTool.name(), "CronList");
    }

    #[test]
    fn test_properties() {
        assert!(CronListTool.is_read_only());
        assert!(CronListTool.is_concurrent_safe());
        assert!(CronListTool.should_defer());
        assert_eq!(
            CronListTool.search_hint(),
            Some("list all scheduled cron jobs")
        );
    }

    #[tokio::test]
    async fn test_list_empty() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = CronListTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        let jobs = parsed["jobs"].as_array().unwrap();
        assert!(jobs.is_empty());
    }
}
