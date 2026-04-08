use async_trait::async_trait;

use crate::services::worktree::WorktreeService;
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct EnterWorktreeTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "EnterWorktree".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for EnterWorktreeTool {
    fn name(&self) -> &str {
        "EnterWorktree"
    }

    fn description(&self) -> &str {
        "Create an isolated git worktree and switch into it for parallel development."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Slug for the worktree (e.g. 'my-feature'). Defaults to first 8 chars of session_id."
                }
            }
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // Check not already in a worktree
        let session_id = {
            let state = ctx.app_state.read().await;
            if state.current_worktree_session.is_some() {
                return Err(tool_err("Already in a worktree session"));
            }
            state.session_id.clone()
        };

        // Derive slug
        let slug = match ctx.input.get("name").and_then(|v| v.as_str()) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => session_id.chars().take(8).collect::<String>(),
        };

        WorktreeService::validate_slug(&slug).map_err(|e| tool_err(e))?;

        let session = WorktreeService::create(&ctx.cwd, &slug, &session_id)
            .await
            .map_err(|e| tool_err(e))?;

        let worktree_path = session.worktree_path.to_string_lossy().to_string();
        let worktree_branch = session.worktree_branch.clone();

        // Store session in app state
        {
            let mut state = ctx.app_state.write().await;
            state.current_worktree_session = Some(session);
        }

        Ok(ToolResult {
            content: serde_json::json!({
                "worktreePath": worktree_path,
                "worktreeBranch": worktree_branch,
                "message": format!("Created worktree at {worktree_path}")
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
        Some("create an isolated git worktree and switch into it")
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

    #[test]
    fn test_name() {
        assert_eq!(EnterWorktreeTool.name(), "EnterWorktree");
    }

    #[test]
    fn test_schema() {
        let schema = EnterWorktreeTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["name"].is_object());
    }

    #[test]
    fn test_should_defer() {
        assert!(EnterWorktreeTool.should_defer());
    }

    #[tokio::test]
    async fn test_already_in_worktree() {
        let ctx = make_ctx(serde_json::json!({}));

        ctx.app_state.write().await.current_worktree_session =
            Some(crate::services::worktree::WorktreeSession {
                original_cwd: PathBuf::from("/tmp"),
                worktree_path: PathBuf::from("/tmp/wt"),
                worktree_name: "existing".into(),
                worktree_branch: None,
                original_branch: None,
                original_head_commit: None,
                session_id: "test".into(),
            });

        let result = EnterWorktreeTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Already in a worktree"), "Error: {}", err);
    }
}
