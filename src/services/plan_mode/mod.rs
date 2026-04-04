use std::path::PathBuf;

use anyhow::Result;

use crate::state::AppState;
use crate::types::permissions::PermissionMode;

pub struct PlanModeService;

impl PlanModeService {
    /// Enter plan mode: stash current mode on permission context, switch to Plan
    pub fn enter(app_state: &mut AppState) -> Result<()> {
        if app_state.tool_permission_context.mode == PermissionMode::Plan {
            anyhow::bail!("Already in plan mode");
        }
        app_state.tool_permission_context.pre_plan_mode =
            Some(app_state.tool_permission_context.mode.clone());
        app_state.tool_permission_context.mode = PermissionMode::Plan;
        Ok(())
    }

    /// Exit plan mode: restore previous mode from permission context
    pub fn exit(app_state: &mut AppState) -> Result<()> {
        if app_state.tool_permission_context.mode != PermissionMode::Plan {
            anyhow::bail!("Not in plan mode");
        }
        if let Some(prev) = app_state.tool_permission_context.pre_plan_mode.take() {
            app_state.tool_permission_context.mode = prev;
        } else {
            app_state.tool_permission_context.mode = PermissionMode::Default;
        }
        Ok(())
    }

    /// Compute plan file path
    pub fn plan_file_path(session_id: &str, agent_id: Option<&str>) -> PathBuf {
        let base = crate::utils::paths::sessions_dir().join(session_id);
        match agent_id {
            Some(id) => base.join("agents").join(id).join("plan.md"),
            None => base.join("plan.md"),
        }
    }

    /// Read plan from file (None if doesn't exist)
    pub fn read_plan(session_id: &str, agent_id: Option<&str>) -> Result<Option<String>> {
        let path = Self::plan_file_path(session_id, agent_id);
        if path.exists() {
            Ok(Some(std::fs::read_to_string(&path)?))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enter_plan_mode() {
        let mut state = AppState::default();
        assert_eq!(state.tool_permission_context.mode, PermissionMode::Default);

        PlanModeService::enter(&mut state).unwrap();
        assert_eq!(state.tool_permission_context.mode, PermissionMode::Plan);
        assert_eq!(
            state.tool_permission_context.pre_plan_mode,
            Some(PermissionMode::Default)
        );
    }

    #[test]
    fn test_enter_already_in_plan_mode() {
        let mut state = AppState::default();
        state.tool_permission_context.mode = PermissionMode::Plan;

        let result = PlanModeService::enter(&mut state);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Already in plan mode"));
    }

    #[test]
    fn test_exit_plan_mode() {
        let mut state = AppState::default();
        PlanModeService::enter(&mut state).unwrap();
        assert_eq!(state.tool_permission_context.mode, PermissionMode::Plan);

        PlanModeService::exit(&mut state).unwrap();
        assert_eq!(state.tool_permission_context.mode, PermissionMode::Default);
        assert_eq!(state.tool_permission_context.pre_plan_mode, None);
    }

    #[test]
    fn test_exit_not_in_plan_mode() {
        let mut state = AppState::default();

        let result = PlanModeService::exit(&mut state);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Not in plan mode"));
    }

    #[test]
    fn test_plan_file_path() {
        let path = PlanModeService::plan_file_path("sess-123", None);
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("sess-123"));
        assert!(path_str.ends_with("plan.md"));
    }

    #[test]
    fn test_plan_file_path_agent() {
        let path = PlanModeService::plan_file_path("sess-123", Some("agent-1"));
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("sess-123"));
        assert!(path_str.contains("agents/agent-1"));
        assert!(path_str.ends_with("plan.md"));
    }
}
