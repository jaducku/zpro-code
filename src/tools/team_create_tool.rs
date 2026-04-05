use async_trait::async_trait;

use crate::services::coordinator::is_agent_swarms_enabled;
use crate::services::team::TeamService;
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::AtomError;

pub struct TeamCreateTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "TeamCreate".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for TeamCreateTool {
    fn name(&self) -> &str {
        "TeamCreate"
    }

    fn description(&self) -> &str {
        "Create a new team for coordinating multiple agents."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "team_name": {
                    "type": "string",
                    "description": "Name for the new team to create"
                },
                "description": {
                    "type": "string",
                    "description": "Team description/purpose"
                }
            },
            "required": ["team_name"]
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        if is_agent_swarms_enabled() {
            return ValidationResult::Ok;
        }
        if let Ok(state) = ctx.app_state.try_read() {
            if !state.swarm_agents.is_empty() {
                return ValidationResult::Ok;
            }
        }
        ValidationResult::Disabled("Agent swarms are not enabled".into())
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        if !is_agent_swarms_enabled() {
            return Err(tool_err("Agent swarms are not enabled"));
        }

        let team_name = ctx
            .input
            .get("team_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required parameter: 'team_name'"))?;

        if team_name.trim().is_empty() {
            return Err(tool_err("team_name must not be empty"));
        }

        let description = ctx.input.get("description").and_then(|v| v.as_str());

        // Check if already leading a team
        {
            let state = ctx.app_state.read().await;
            if state.team_name.is_some() {
                return Err(tool_err(
                    "Already leading a team. A leader can only manage one team at a time. Use TeamDelete to end the current team before creating a new one.",
                ));
            }
        }

        let session_id = ctx.app_state.read().await.session_id.clone();
        let svc = TeamService::new();
        let (config, config_path) = svc
            .create(team_name, description, Some(&session_id))
            .map_err(|e| tool_err(format!("Failed to create team: {}", e)))?;

        // Update app state
        {
            let mut state = ctx.app_state.write().await;
            state.team_name = Some(team_name.to_string());
        }

        let result = serde_json::json!({
            "team_name": config.name,
            "team_file_path": config_path.to_string_lossy(),
            "lead_agent_id": config.lead_agent_id,
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
        Some("create a multi-agent team for collaborative work")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(TeamCreateTool.name(), "TeamCreate");
    }

    #[test]
    fn test_schema() {
        let schema = TeamCreateTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["team_name"].is_object());
        assert!(schema["properties"]["description"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("team_name")));
    }

    #[test]
    fn test_should_defer() {
        assert!(TeamCreateTool.should_defer());
    }

    #[tokio::test]
    async fn test_create_sets_team_name() {
        // Enable swarms for this test
        std::env::set_var("ATOM_CODE_AGENT_SWARMS", "1");

        let ctx = ToolUseContext::test(serde_json::json!({
            "team_name": "test-team",
            "description": "A test team"
        }));
        let app_state = ctx.app_state.clone();

        let result = TeamCreateTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["team_name"], "test-team");
        assert_eq!(parsed["lead_agent_id"], "team-lead@test-team");

        let state = app_state.read().await;
        assert_eq!(state.team_name.as_deref(), Some("test-team"));

        // Cleanup
        std::env::remove_var("ATOM_CODE_AGENT_SWARMS");
    }

    #[tokio::test]
    async fn test_already_leading() {
        std::env::set_var("ATOM_CODE_AGENT_SWARMS", "1");

        let ctx = ToolUseContext::test(serde_json::json!({
            "team_name": "second-team"
        }));
        {
            let mut state = ctx.app_state.write().await;
            state.team_name = Some("first-team".to_string());
        }

        let result = TeamCreateTool.execute(ctx).await;
        assert!(result.is_err());

        std::env::remove_var("ATOM_CODE_AGENT_SWARMS");
    }
}
