use async_trait::async_trait;

use crate::services::worktree::WorktreeService;
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::AtomError;

pub struct ExitWorktreeTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "ExitWorktree".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for ExitWorktreeTool {
    fn name(&self) -> &str {
        "ExitWorktree"
    }

    fn description(&self) -> &str {
        "Exit a worktree session and return to the original directory."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["keep", "remove"],
                    "description": "Whether to keep or remove the worktree"
                },
                "discard_changes": {
                    "type": "boolean",
                    "description": "Force remove with uncommitted changes"
                }
            },
            "required": ["action"]
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        if let Ok(state) = ctx.app_state.try_read() {
            if state.current_worktree_session.is_some() {
                return ValidationResult::Ok;
            }
        }
        ValidationResult::Disabled("Not in a worktree session".into())
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // 1. Check current_worktree_session is Some
        let session = {
            let state = ctx.app_state.read().await;
            match &state.current_worktree_session {
                Some(s) => s.clone(),
                None => return Err(tool_err("Not in a worktree session")),
            }
        };

        // 2. Extract action (required)
        let action = ctx
            .input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required field: action"))?;

        if action != "keep" && action != "remove" {
            return Err(tool_err(format!(
                "Invalid action '{}': must be 'keep' or 'remove'",
                action
            )));
        }

        let discard_changes = ctx
            .input
            .get("discard_changes")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let original_cwd = session.original_cwd.to_string_lossy().to_string();
        let worktree_path = session.worktree_path.to_string_lossy().to_string();
        let worktree_branch = session.worktree_branch.clone().unwrap_or_default();

        let mut discarded_files: u32 = 0;
        let mut discarded_commits: u32 = 0;

        if action == "remove" {
            // Count uncommitted changes
            let uncommitted = WorktreeService::count_uncommitted(&session.worktree_path)
                .await
                .unwrap_or(0);

            // Count new commits since original head
            let new_commits = if let Some(ref head) = session.original_head_commit {
                WorktreeService::count_new_commits(&session.worktree_path, head)
                    .await
                    .unwrap_or(0)
            } else {
                0
            };

            // Safety gate: if there are changes and discard_changes is not true, error
            if (uncommitted > 0 || new_commits > 0) && !discard_changes {
                return Ok(ToolResult {
                    content: serde_json::json!({
                        "error": "Worktree has unsaved work",
                        "uncommittedFiles": uncommitted,
                        "newCommits": new_commits,
                        "hint": "Set discard_changes=true to force remove, or use action='keep'"
                    })
                    .to_string(),
                    is_error: true,
                    content_blocks: None,
                });
            }

            let result = WorktreeService::remove(&session)
                .await
                .map_err(|e| tool_err(e))?;

            discarded_files = result.discarded_files;
            discarded_commits = result.discarded_commits;
        }

        // Clear worktree session
        {
            let mut state = ctx.app_state.write().await;
            state.current_worktree_session = None;
        }

        let message = if action == "remove" {
            format!("Exited and removed worktree at {worktree_path}")
        } else {
            format!("Exited worktree (kept at {worktree_path})")
        };

        Ok(ToolResult {
            content: serde_json::json!({
                "action": action,
                "originalCwd": original_cwd,
                "worktreePath": worktree_path,
                "worktreeBranch": worktree_branch,
                "discardedFiles": discarded_files,
                "discardedCommits": discarded_commits,
                "message": message,
            })
            .to_string(),
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
        Some("exit a worktree session and return to the original directory")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;
    use std::path::PathBuf;

    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        ToolUseContext::test(input)
    }

    fn dummy_session() -> crate::services::worktree::WorktreeSession {
        crate::services::worktree::WorktreeSession {
            original_cwd: PathBuf::from("/original/path"),
            worktree_path: PathBuf::from("/.claude/worktrees/my-slug"),
            worktree_name: "my-slug".into(),
            worktree_branch: Some("worktree-my-slug".into()),
            original_branch: Some("main".into()),
            original_head_commit: Some("abc123".into()),
            session_id: "test-session".into(),
        }
    }

    #[test]
    fn test_name() {
        assert_eq!(ExitWorktreeTool.name(), "ExitWorktree");
    }

    #[test]
    fn test_schema() {
        let schema = ExitWorktreeTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["discard_changes"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("action")));
        assert!(!required.contains(&serde_json::json!("discard_changes")));
    }

    #[test]
    fn test_should_defer() {
        assert!(ExitWorktreeTool.should_defer());
        assert!(!ExitWorktreeTool.is_read_only());
    }

    #[tokio::test]
    async fn test_not_in_worktree() {
        let ctx = make_ctx(serde_json::json!({ "action": "keep" }));
        let result = ExitWorktreeTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Not in a worktree session"), "Error: {}", err);
    }

    #[tokio::test]
    async fn test_missing_action() {
        let ctx = make_ctx(serde_json::json!({}));
        ctx.app_state.write().await.current_worktree_session = Some(dummy_session());
        let result = ExitWorktreeTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Missing required field: action"),
            "Error: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_invalid_action() {
        let ctx = make_ctx(serde_json::json!({ "action": "invalid" }));
        ctx.app_state.write().await.current_worktree_session = Some(dummy_session());
        let result = ExitWorktreeTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid action"), "Error: {}", err);
    }
}
