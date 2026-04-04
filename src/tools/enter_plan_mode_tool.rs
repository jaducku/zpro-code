use async_trait::async_trait;

use crate::services::plan_mode::PlanModeService;
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::permissions::PermissionMode;
use crate::types::AtomError;

pub struct EnterPlanModeTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "EnterPlanMode".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for EnterPlanModeTool {
    fn name(&self) -> &str {
        "EnterPlanMode"
    }

    fn description(&self) -> &str {
        "Enter plan mode to design an approach before writing code. In plan mode, \
         only read-only tools are available."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        if let Ok(state) = ctx.app_state.try_read() {
            if state.coordinator_mode {
                return ValidationResult::Disabled("Cannot enter plan mode in coordinator".into());
            }
            if state.tool_permission_context.mode == PermissionMode::Plan {
                return ValidationResult::Disabled("Already in plan mode".into());
            }
            ValidationResult::Ok
        } else {
            // Cannot acquire read lock; conservatively disable
            ValidationResult::Disabled("Unable to check app state".into())
        }
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // Agent guard: coordinators cannot enter plan mode
        {
            let state = ctx.app_state.read().await;
            if state.coordinator_mode {
                return Err(tool_err("Cannot enter plan mode in coordinator"));
            }
        }

        let mut state = ctx.app_state.write().await;
        PlanModeService::enter(&mut state).map_err(|e| tool_err(e.to_string()))?;

        Ok(ToolResult {
            content: serde_json::json!({
                "message": "Entered plan mode. Only read-only tools are available until you exit plan mode."
            })
            .to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    // Conceptually read-only: entering plan mode restricts future tool use but
    // does not itself modify files or external state. The write lock on AppState
    // is an internal bookkeeping detail.
    fn is_read_only(&self) -> bool {
        true
    }

    fn should_defer(&self) -> bool {
        true
    }

    fn search_hint(&self) -> Option<&str> {
        Some("switch to plan mode to design an approach before coding")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;
    use std::sync::Arc;

    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        ToolUseContext::test(input)
    }

    #[test]
    fn test_name() {
        assert_eq!(EnterPlanModeTool.name(), "EnterPlanMode");
    }

    #[test]
    fn test_schema() {
        let schema = EnterPlanModeTool.input_schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_should_defer() {
        assert!(EnterPlanModeTool.should_defer());
    }

    #[tokio::test]
    async fn test_enter_plan_mode() {
        let ctx = make_ctx(serde_json::json!({}));
        let app_state = ctx.app_state.clone();

        let result = EnterPlanModeTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("plan mode"));

        let state = app_state.read().await;
        assert_eq!(state.tool_permission_context.mode, PermissionMode::Plan);
    }

    #[tokio::test]
    async fn test_already_in_plan_mode() {
        let app_state = crate::state::AppState {
            tool_permission_context: crate::types::permissions::ToolPermissionContext {
                mode: PermissionMode::Plan,
                ..Default::default()
            },
            ..Default::default()
        };
        let app_state = Arc::new(tokio::sync::RwLock::new(app_state));

        let mut ctx = make_ctx(serde_json::json!({}));
        ctx.app_state = app_state;

        let result = EnterPlanModeTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Already in plan mode"), "Error: {}", err);
    }
}
