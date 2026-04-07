use async_trait::async_trait;

use crate::services::coordinator::is_agent_swarms_enabled;
use crate::services::plan_mode::PlanModeService;
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::permissions::{PermissionMode, PermissionRule};
use crate::types::AtomError;

pub struct ExitPlanModeTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "ExitPlanMode".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn name(&self) -> &str {
        "ExitPlanMode"
    }

    fn description(&self) -> &str {
        "Present plan for approval and start coding. Only available in plan mode."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "allowedPrompts": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "tool": { "type": "string" },
                            "prompt": { "type": "string" }
                        }
                    },
                    "description": "Bash commands to allow during plan execution"
                }
            }
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        if let Ok(state) = ctx.app_state.try_read() {
            if state.tool_permission_context.mode != PermissionMode::Plan {
                return ValidationResult::Disabled("Not in plan mode".into());
            }
        }
        ValidationResult::Ok
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // 1. Check plan mode (defensive duplicate of can_use)
        let session_id = {
            let state = ctx.app_state.read().await;
            if state.tool_permission_context.mode != PermissionMode::Plan {
                return Err(tool_err("Not in plan mode"));
            }
            state.session_id.clone()
        };

        // 2. Read plan file
        let plan = PlanModeService::read_plan(&session_id, None)
            .map_err(|e| tool_err(format!("Failed to read plan: {}", e)))?;
        let file_path = PlanModeService::plan_file_path(&session_id, None);

        // 3. Process allowedPrompts → add to always_allow_rules
        if let Some(prompts) = ctx.input.get("allowedPrompts").and_then(|v| v.as_array()) {
            let mut state = ctx.app_state.write().await;
            for prompt_obj in prompts {
                let tool_name = prompt_obj
                    .get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Bash")
                    .to_string();
                let rule_content = prompt_obj
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                state
                    .tool_permission_context
                    .always_allow_rules
                    .push(PermissionRule {
                        tool_name,
                        rule_content,
                    });
            }
        }

        // 4. Exit plan mode (restore permission)
        {
            let mut state = ctx.app_state.write().await;
            PlanModeService::exit(&mut state)
                .map_err(|e| tool_err(format!("{}", e)))?;
        }

        // 5. Check if Agent tool available + swarms enabled
        let has_task_tool =
            is_agent_swarms_enabled() && ctx.tools.iter().any(|t| t.name() == "Agent");

        // 6. Check if agent context
        let is_agent = ctx.app_state.read().await.team_name.is_some();

        // 7. Return output
        let response = serde_json::json!({
            "plan": plan,
            "isAgent": is_agent,
            "filePath": file_path.display().to_string(),
            "hasTaskTool": has_task_tool,
            "awaitingLeaderApproval": false,
        });

        Ok(ToolResult {
            content: response.to_string(),
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
        Some("present plan for approval and start coding (plan mode only)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(ExitPlanModeTool.name(), "ExitPlanMode");
    }

    #[test]
    fn test_schema() {
        let schema = ExitPlanModeTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["allowedPrompts"].is_object());
    }

    #[test]
    fn test_should_defer() {
        assert!(ExitPlanModeTool.should_defer());
        assert!(!ExitPlanModeTool.is_read_only());
    }

    #[tokio::test]
    async fn test_exit_plan_mode() {
        let tool = ExitPlanModeTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        // Enter plan mode first
        {
            let mut state = ctx.app_state.write().await;
            state.tool_permission_context.mode = PermissionMode::Plan;
            state.tool_permission_context.pre_plan_mode = Some(PermissionMode::Default);
        }
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["isAgent"], false);
        assert_eq!(parsed["awaitingLeaderApproval"], false);
    }

    #[tokio::test]
    async fn test_exit_not_in_plan_mode() {
        let tool = ExitPlanModeTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_exit_with_allowed_prompts() {
        let tool = ExitPlanModeTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "allowedPrompts": [
                { "tool": "Bash", "prompt": "cargo test" },
                { "tool": "Bash", "prompt": "cargo build" }
            ]
        }));
        {
            let mut state = ctx.app_state.write().await;
            state.tool_permission_context.mode = PermissionMode::Plan;
            state.tool_permission_context.pre_plan_mode = Some(PermissionMode::Default);
        }
        let app_state = ctx.app_state.clone();
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        // Verify rules were added
        let state = app_state.read().await;
        assert_eq!(state.tool_permission_context.mode, PermissionMode::Default);
        assert_eq!(state.tool_permission_context.always_allow_rules.len(), 2);
    }
}
