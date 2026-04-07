use async_trait::async_trait;

use crate::services::team::{active_member_count, TeamService};
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::AtomError;

pub struct TeamDeleteTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "TeamDelete".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for TeamDeleteTool {
    fn name(&self) -> &str {
        "TeamDelete"
    }

    fn description(&self) -> &str {
        "Clean up team and task directories when the swarm is complete."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        if let Ok(state) = ctx.app_state.try_read() {
            if state.team_name.is_some() {
                return ValidationResult::Ok;
            }
        }
        ValidationResult::Disabled("Not currently leading a team".into())
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let team_name = {
            let state = ctx.app_state.read().await;
            state.team_name.clone()
        };

        let team_name = team_name
            .ok_or_else(|| tool_err("Not currently leading a team. Nothing to delete."))?;

        let svc = TeamService::new();

        // Check for active members
        if let Some(config) = svc
            .load(&team_name)
            .map_err(|e| tool_err(format!("Failed to load team: {}", e)))?
        {
            let active = active_member_count(&config);
            if active > 0 {
                let names: Vec<&str> = config
                    .members
                    .iter()
                    .filter(|m| m.is_active && m.agent_id != config.lead_agent_id)
                    .map(|m| m.name.as_str())
                    .collect();
                return Ok(ToolResult {
                    content: serde_json::json!({
                        "success": false,
                        "message": format!(
                            "Cannot delete team with {} active member(s): {}. Use requestShutdown to terminate teammates first.",
                            active,
                            names.join(", ")
                        ),
                        "team_name": team_name,
                    })
                    .to_string(),
                    is_error: false,
                    content_blocks: None,
                });
            }
        }

        svc.delete(&team_name)
            .map_err(|e| tool_err(format!("Failed to delete team: {}", e)))?;

        // Clear app state
        {
            let mut state = ctx.app_state.write().await;
            state.team_name = None;
        }

        let result = serde_json::json!({
            "success": true,
            "message": format!("Cleaned up directories for team \"{}\"", team_name),
            "team_name": team_name,
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
        Some("disband a team and clean up resources")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(TeamDeleteTool.name(), "TeamDelete");
    }

    #[test]
    fn test_schema() {
        let schema = TeamDeleteTool.input_schema();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn test_should_defer() {
        assert!(TeamDeleteTool.should_defer());
    }

    #[tokio::test]
    async fn test_not_in_team() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = TeamDeleteTool.execute(ctx).await;
        assert!(result.is_err());
    }
}
