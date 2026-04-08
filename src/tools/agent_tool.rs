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
                content:
                    "Maximum agent nesting depth (3) reached. Cannot spawn further sub-agents."
                        .into(),
                is_error: true,
                content_blocks: None,
            });
        }

        let mut params = crate::services::agent_runtime::AgentSpawnParams::from_input(&ctx.input)?;
        let subagent_type = ctx.input.get("subagent_type").and_then(|v| v.as_str());

        // Fork path: subagent_type omitted + fork enabled + not already inside a fork
        let is_fork_path = subagent_type.is_none()
            && crate::services::fork_subagent::is_fork_enabled(&ctx.app_state).await
            && !crate::services::fork_subagent::is_inside_fork(
                ctx.query_source.as_deref(),
                ctx.messages.as_ref(),
            );

        if is_fork_path {
            // Extract last assistant message for fork message construction
            if let Some(assistant_msg) = ctx
                .messages
                .iter()
                .rev()
                .find(|m| m.role == crate::types::message::Role::Assistant)
            {
                let fork_msgs = crate::services::fork_subagent::build_forked_messages(
                    &params.prompt,
                    assistant_msg,
                );
                params.is_fork = true;
                params.fork_messages = Some(fork_msgs);
                params.query_source =
                    Some(crate::services::fork_subagent::FORK_QUERY_SOURCE.to_string());
            }
            // If no assistant message found, fall through to normal path
        }

        // Lookup builtin/custom agent from registry if subagent_type specified
        if let Some(agent_name) = subagent_type {
            let (agent_def, override_model) = {
                let state = ctx.app_state.read().await;
                let def = state.agent_registry.get(agent_name).cloned();
                let override_model = def.as_ref().and_then(|d| {
                    resolve_builtin_model(&state.agent_config, agent_name, &d.model)
                });
                (def, override_model)
            };

            if let Some(mut def) = agent_def {
                // Model: config override → code default → None (inherit)
                if let Some(override_m) = override_model {
                    def.model = Some(override_m);
                }

                if params.model.is_none() {
                    params.model = def.model.clone();
                }
                if params.isolation.is_none() {
                    params.isolation = def.isolation.clone();
                }
                params.query_source = Some(def.query_source.clone());
                params.agent_def = Some(def);
            }
            // If agent not found, fallback to general agent (no warning)
        }

        if params.run_in_background {
            crate::services::agent_runtime::run_background(params, &ctx).await
        } else {
            let description = params.description.clone();
            let agent_type = params.agent_type.clone();
            let result = crate::services::agent_runtime::run_foreground(params, &ctx).await?;
            // Send AgentCompleted event so TUI shows agent with stats
            let _ = ctx.event_tx.send(crate::query::QueryEvent::AgentCompleted {
                agent_id: format!("fg-{}", uuid::Uuid::new_v4()),
                agent_type,
                description,
                result: Some(result.output.clone()),
                is_error: result.is_error,
                tool_count: result.tool_count,
                token_count: result.token_count,
                duration_secs: result.duration_secs,
            });
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

/// Resolve builtin agent model with config override + code default.
/// Priority: config.builtin_models.{name} → code_default → None
fn resolve_builtin_model(
    agent_config: &crate::state::app_state::AgentConfig,
    agent_name: &str,
    code_default: &Option<String>,
) -> Option<String> {
    match agent_name {
        "general-purpose" => agent_config
            .builtin_models
            .general_purpose
            .clone()
            .or_else(|| code_default.clone()),
        "Explore" => agent_config
            .builtin_models
            .explore
            .clone()
            .or_else(|| code_default.clone()),
        "Plan" => agent_config
            .builtin_models
            .plan
            .clone()
            .or_else(|| code_default.clone()),
        "Verification" => agent_config
            .builtin_models
            .verification
            .clone()
            .or_else(|| code_default.clone()),
        "claude-code-guide" => agent_config
            .builtin_models
            .claude_code_guide
            .clone()
            .or_else(|| code_default.clone()),
        _ => code_default.clone(),
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

    #[tokio::test]
    async fn test_agent_tool_fork_path_detection_when_enabled() {
        let tool = AgentTool;
        let ctx = make_ctx(serde_json::json!({"prompt": "test"}));
        // Enable fork in settings
        {
            let mut state = ctx.app_state.write().await;
            state.settings.fork_subagent = Some(true);
        }
        // With no subagent_type and fork enabled, should take fork path
        // Will fail at query() (NoopProvider) but should not be rejected for wrong reasons
        let result = tool.execute(ctx).await;
        match result {
            Ok(r) => assert!(!r.content.contains("nesting depth")),
            Err(_) => {} // query failure expected with NoopProvider
        }
    }
}
