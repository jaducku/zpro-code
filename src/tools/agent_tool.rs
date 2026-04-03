use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::permissions::PermissionMeta;

pub struct AgentTool;

#[async_trait]
impl Tool for AgentTool {
    fn name(&self) -> &str {
        "Agent"
    }

    fn description(&self) -> &str {
        "Launch a new agent to handle complex, multi-step tasks autonomously."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The task description for the sub-agent"
                },
                "description": {
                    "type": "string",
                    "description": "A short 3-5 word summary of the task"
                },
                "subagent_type": {
                    "type": "string",
                    "description": "The type of agent to spawn"
                },
                "model": {
                    "type": "string",
                    "description": "Model override for the sub-agent"
                },
                "run_in_background": {
                    "type": "boolean",
                    "description": "Whether to run the agent asynchronously"
                },
                "name": {
                    "type": "string",
                    "description": "Named agent identifier for continuation"
                },
                "isolation": {
                    "type": "string",
                    "enum": ["worktree"],
                    "description": "Isolation mode for the agent"
                }
            },
            "required": ["prompt"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let prompt = ctx
            .input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Agent".into(),
                message: "Missing 'prompt' parameter".into(),
            })?;

        let description = ctx
            .input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("sub-agent task");

        // Stub: full implementation requires coordinator infrastructure
        // (query engine fork, tool pool, async lifecycle management)
        Ok(ToolResult {
            content: format!(
                "Agent requested: {} ({}). \
                 Sub-agent spawning is not yet implemented.",
                description, prompt
            ),
            is_error: false,
        })
    }

    fn can_use(&self, _ctx: &ToolUseContext) -> ValidationResult {
        ValidationResult::Ok
    }

    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            command: input
                .get("prompt")
                .and_then(|v| v.as_str())
                .map(String::from),
            file_path: None,
            url: None,
        }
    }

    fn is_read_only(&self) -> bool {
        false
    }

    fn is_concurrent_safe(&self) -> bool {
        false
    }

    fn aliases(&self) -> &[&str] {
        &[]
    }

    fn is_mcp(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        ToolUseContext {
            input,
            abort_signal: CancellationToken::new(),
            progress_tx: tx,
        }
    }

    #[test]
    fn test_agent_tool_name() {
        let tool = AgentTool;
        assert_eq!(tool.name(), "Agent");
    }

    #[test]
    fn test_agent_tool_schema() {
        let tool = AgentTool;
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");

        let props = &schema["properties"];
        assert!(props["prompt"].is_object());
        assert!(props["description"].is_object());
        assert!(props["subagent_type"].is_object());
        assert!(props["model"].is_object());
        assert!(props["run_in_background"].is_object());
        assert!(props["isolation"].is_object());

        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "prompt");
    }

    #[test]
    fn test_agent_tool_is_not_read_only() {
        let tool = AgentTool;
        assert!(!tool.is_read_only());
    }

    #[test]
    fn test_agent_tool_not_concurrent_safe() {
        let tool = AgentTool;
        assert!(!tool.is_concurrent_safe());
    }

    #[tokio::test]
    async fn test_agent_tool_execute_stub() {
        let tool = AgentTool;
        let ctx = make_ctx(serde_json::json!({
            "prompt": "Fix the build errors",
            "description": "fix build"
        }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("fix build"));
        assert!(result.content.contains("not yet implemented"));
    }

    #[tokio::test]
    async fn test_agent_tool_missing_prompt() {
        let tool = AgentTool;
        let ctx = make_ctx(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
