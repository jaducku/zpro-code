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
        // Enforce max nesting depth
        if ctx.agent_depth >= 3 {
            return Ok(ToolResult {
                content: "Maximum agent nesting depth (3) reached. Cannot spawn further sub-agents.".into(),
                is_error: true,
                content_blocks: None,
            });
        }

        let params = crate::services::agent_runtime::AgentSpawnParams::from_input(&ctx.input)?;

        if params.run_in_background {
            crate::services::agent_runtime::run_background(params, &ctx).await
        } else {
            let result = crate::services::agent_runtime::run_foreground(params, &ctx).await?;
            Ok(ToolResult {
                content: result.output,
                is_error: result.is_error,
                content_blocks: None,
            })
        }
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

    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        ToolUseContext::test(input)
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
    async fn test_agent_tool_missing_prompt() {
        let tool = AgentTool;
        let ctx = make_ctx(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_agent_tool_max_depth_rejected() {
        let tool = AgentTool;
        let mut ctx = make_ctx(serde_json::json!({"prompt": "test"}));
        ctx.agent_depth = 3;
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("nesting depth"));
    }

    #[tokio::test]
    async fn test_agent_tool_depth_2_allowed() {
        let tool = AgentTool;
        let mut ctx = make_ctx(serde_json::json!({"prompt": "test"}));
        ctx.agent_depth = 2;
        // This will try to run a real query which will fail with NoopProvider,
        // but should NOT be rejected for depth
        let result = tool.execute(ctx).await;
        match result {
            Ok(r) => assert!(!r.content.contains("nesting depth")),
            Err(_) => {} // query failure is expected with NoopProvider
        }
    }
}
