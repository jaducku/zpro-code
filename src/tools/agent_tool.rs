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
                message: "Missing required 'prompt' parameter".into(),
            })?;

        let description = ctx
            .input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("agent task");

        let subagent_type = ctx.input.get("subagent_type").and_then(|v| v.as_str());
        let model = ctx.input.get("model").and_then(|v| v.as_str());
        let run_in_background = ctx
            .input
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let isolation = ctx.input.get("isolation").and_then(|v| v.as_str());

        let response = serde_json::json!({
            "status": "agent_dispatched",
            "prompt": prompt,
            "description": description,
            "subagent_type": subagent_type,
            "model": model,
            "run_in_background": run_in_background,
            "isolation": isolation,
            "note": "Agent dispatched. The agent will process this task and return results."
        });

        Ok(ToolResult {
            content: serde_json::to_string_pretty(&response).map_err(|e| {
                crate::types::AtomError::Tool {
                    tool_name: "Agent".into(),
                    message: format!("Failed to serialize response: {}", e),
                }
            })?,
            is_error: false,
            content_blocks: None,
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
    async fn test_agent_tool_execute_dispatch() {
        let tool = AgentTool;
        let ctx = make_ctx(serde_json::json!({
            "prompt": "Fix the build errors",
            "description": "fix build"
        }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["status"], "agent_dispatched");
        assert_eq!(parsed["prompt"], "Fix the build errors");
        assert_eq!(parsed["description"], "fix build");
        assert_eq!(parsed["run_in_background"], false);
        assert!(parsed["subagent_type"].is_null());
        assert!(parsed["model"].is_null());
        assert!(parsed["isolation"].is_null());
    }

    #[tokio::test]
    async fn test_agent_tool_execute_all_fields() {
        let tool = AgentTool;
        let ctx = make_ctx(serde_json::json!({
            "prompt": "Refactor the parser",
            "description": "refactor parser",
            "subagent_type": "research",
            "model": "claude-sonnet-4-20250514",
            "run_in_background": true,
            "isolation": "worktree"
        }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["status"], "agent_dispatched");
        assert_eq!(parsed["subagent_type"], "research");
        assert_eq!(parsed["model"], "claude-sonnet-4-20250514");
        assert_eq!(parsed["run_in_background"], true);
        assert_eq!(parsed["isolation"], "worktree");
    }

    #[tokio::test]
    async fn test_agent_tool_execute_defaults() {
        let tool = AgentTool;
        let ctx = make_ctx(serde_json::json!({
            "prompt": "Do something"
        }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["description"], "agent task");
        assert_eq!(parsed["run_in_background"], false);
    }

    #[tokio::test]
    async fn test_agent_tool_missing_prompt() {
        let tool = AgentTool;
        let ctx = make_ctx(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
