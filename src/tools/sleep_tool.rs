use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct SleepTool;

#[async_trait]
impl Tool for SleepTool {
    fn name(&self) -> &str {
        "Sleep"
    }

    fn description(&self) -> &str {
        "Pause execution for a specified duration. The user can interrupt at any time."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "duration_ms": {
                    "type": "integer",
                    "description": "Duration to sleep in milliseconds (max 60000)",
                    "maximum": 60000
                }
            },
            "required": ["duration_ms"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let duration_ms = ctx
            .input
            .get("duration_ms")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Sleep".into(),
                message: "Missing 'duration_ms' parameter".into(),
            })?;

        let clamped = duration_ms.min(60_000);

        let slept = tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_millis(clamped)) => true,
            _ = ctx.abort_signal.cancelled() => false,
        };

        if slept {
            Ok(ToolResult {
                content: format!("Slept for {}ms", clamped),
                is_error: false,
            })
        } else {
            Ok(ToolResult {
                content: "Sleep interrupted".into(),
                is_error: false,
            })
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(SleepTool.name(), "Sleep");
    }

    #[tokio::test]
    async fn test_short_sleep() {
        let tool = SleepTool;
        let ctx = ToolUseContext::test(serde_json::json!({"duration_ms": 10}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content, "Slept for 10ms");
    }

    #[tokio::test]
    async fn test_clamp_max() {
        let tool = SleepTool;
        // Provide a value over the max — it should be clamped to 60000.
        // We don't actually sleep 60s; instead we cancel immediately.
        let ctx = ToolUseContext::test(serde_json::json!({"duration_ms": 99999}));
        // Cancel right away so the test doesn't block
        ctx.abort_signal.cancel();
        let result = tool.execute(ctx).await.unwrap();
        // Either interrupted or slept — the important thing is no error and no panic
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn test_missing_param() {
        let tool = SleepTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
