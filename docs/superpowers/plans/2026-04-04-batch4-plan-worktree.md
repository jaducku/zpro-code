# Batch 4: Plan & Worktree Tools — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 4 plan/worktree tools (EnterPlanMode, ExitPlanMode, EnterWorktree, ExitWorktree) with supporting services.

**Architecture:** Tool-first approach. Plan mode tools modify `ToolPermissionContext.mode`. Worktree tools shell out to `git worktree` via `tokio::process::Command`. Both tool pairs store session state in AppState.

**Tech Stack:** Rust, async-trait, tokio, tokio::process::Command, serde, fs2

**Spec:** `docs/superpowers/specs/2026-04-04-batch4-plan-worktree-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/types/permissions.rs` | Modify | Add `pre_plan_mode` to `ToolPermissionContext` |
| `src/state/app_state.rs` | Modify | Add `current_worktree_session` |
| `src/services/plan_mode/mod.rs` | Create | PlanModeService: enter/exit/read_plan |
| `src/services/worktree/mod.rs` | Create | WorktreeService: create/remove/validate/git ops |
| `src/services/mod.rs` | Modify | Add `pub mod plan_mode; pub mod worktree;` |
| `src/tools/enter_plan_mode_tool.rs` | Create | EnterPlanModeTool |
| `src/tools/exit_plan_mode_tool.rs` | Create | ExitPlanModeTool |
| `src/tools/enter_worktree_tool.rs` | Create | EnterWorktreeTool |
| `src/tools/exit_worktree_tool.rs` | Create | ExitWorktreeTool |
| `src/tools/mod.rs` | Modify | Register 4 new tools |

---

### Task 1: ToolPermissionContext.pre_plan_mode + AppState.current_worktree_session

**Files:**
- Modify: `src/types/permissions.rs`
- Modify: `src/state/app_state.rs`

- [ ] **Step 1: Add `pre_plan_mode` to ToolPermissionContext**

In `src/types/permissions.rs`, add field to `ToolPermissionContext` after `should_avoid_permission_prompts` (line 53):

```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_plan_mode: Option<PermissionMode>,
```

Add `pre_plan_mode: None,` to the `Default` impl.

- [ ] **Step 2: Add test for pre_plan_mode serde**

```rust
#[test]
fn test_permission_context_pre_plan_mode_serde() {
    let mut ctx = ToolPermissionContext::default();
    ctx.pre_plan_mode = Some(PermissionMode::Default);
    let json = serde_json::to_string(&ctx).unwrap();
    assert!(json.contains("pre_plan_mode"));
    let parsed: ToolPermissionContext = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.pre_plan_mode, Some(PermissionMode::Default));
}

#[test]
fn test_permission_context_pre_plan_mode_none_omitted() {
    let ctx = ToolPermissionContext::default();
    let json = serde_json::to_string(&ctx).unwrap();
    assert!(!json.contains("pre_plan_mode"));
}
```

- [ ] **Step 3: Forward-declare WorktreeSession and add to AppState**

In `src/state/app_state.rs`, add after `swarm_agents` fields (the worktree service doesn't exist yet, so use `serde_json::Value` temporarily — will be replaced in Task 4):

```rust
    /// Active worktree session (set by EnterWorktree, cleared by ExitWorktree)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_worktree_session: Option<serde_json::Value>,
```

Add `current_worktree_session: None,` to Default impl.

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code permissions 2>&1 | tail -10`
Run: `cargo test -p atom-code app_state 2>&1 | tail -10`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add src/types/permissions.rs src/state/app_state.rs
git commit -m "feat: add pre_plan_mode to ToolPermissionContext, current_worktree_session to AppState"
```

---

### Task 2: Plan Mode Service + EnterPlanModeTool

**Files:**
- Create: `src/services/plan_mode/mod.rs`
- Modify: `src/services/mod.rs`
- Create: `src/tools/enter_plan_mode_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Create Plan Mode service**

Create `src/services/plan_mode/mod.rs`:

```rust
use std::path::PathBuf;
use anyhow::Result;
use crate::state::AppState;
use crate::types::permissions::PermissionMode;

pub struct PlanModeService;

impl PlanModeService {
    /// Enter plan mode: stash current mode, switch to Plan
    pub fn enter(app_state: &mut AppState) -> Result<()> {
        if app_state.tool_permission_context.mode == PermissionMode::Plan {
            anyhow::bail!("Already in plan mode");
        }
        app_state.tool_permission_context.pre_plan_mode =
            Some(app_state.tool_permission_context.mode.clone());
        app_state.tool_permission_context.mode = PermissionMode::Plan;
        Ok(())
    }

    /// Exit plan mode: restore previous mode
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
        assert!(PlanModeService::enter(&mut state).is_err());
    }

    #[test]
    fn test_exit_plan_mode() {
        let mut state = AppState::default();
        PlanModeService::enter(&mut state).unwrap();
        PlanModeService::exit(&mut state).unwrap();
        assert_eq!(state.tool_permission_context.mode, PermissionMode::Default);
        assert!(state.tool_permission_context.pre_plan_mode.is_none());
    }

    #[test]
    fn test_exit_not_in_plan_mode() {
        let mut state = AppState::default();
        assert!(PlanModeService::exit(&mut state).is_err());
    }

    #[test]
    fn test_plan_file_path() {
        let path = PlanModeService::plan_file_path("sess-123", None);
        assert!(path.to_string_lossy().contains("sess-123"));
        assert!(path.to_string_lossy().ends_with("plan.md"));
    }

    #[test]
    fn test_plan_file_path_agent() {
        let path = PlanModeService::plan_file_path("sess-123", Some("agent-1"));
        assert!(path.to_string_lossy().contains("agents/agent-1"));
    }
}
```

- [ ] **Step 2: Register plan_mode service**

In `src/services/mod.rs`, add:
```rust
pub mod plan_mode;
```

- [ ] **Step 3: Implement EnterPlanModeTool**

Create `src/tools/enter_plan_mode_tool.rs`:

```rust
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
    fn name(&self) -> &str { "EnterPlanMode" }

    fn description(&self) -> &str {
        "Switch to plan mode to design an approach before coding."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // Agent guard: agents cannot enter plan mode
        {
            let state = ctx.app_state.read().await;
            if state.coordinator_mode {
                return Err(tool_err("EnterPlanMode cannot be used in agent/coordinator context"));
            }
        }

        let mut state = ctx.app_state.write().await;
        PlanModeService::enter(&mut state)
            .map_err(|e| tool_err(format!("{}", e)))?;

        let response = serde_json::json!({
            "message": "Entered plan mode. Design your approach before coding. Use ExitPlanMode when your plan is ready."
        });

        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        if let Ok(state) = ctx.app_state.try_read() {
            if state.tool_permission_context.mode == PermissionMode::Plan {
                return ValidationResult::Disabled("Already in plan mode".into());
            }
            if state.coordinator_mode {
                return ValidationResult::Disabled("Not available in agent context".into());
            }
        }
        ValidationResult::Ok
    }

    // Conceptually read-only (permission change is metadata).
    // Note: acquires write lock on app_state to mutate permission mode.
    fn is_read_only(&self) -> bool { true }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> {
        Some("switch to plan mode to design an approach before coding")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(EnterPlanModeTool.is_read_only());
    }

    #[tokio::test]
    async fn test_enter_plan_mode() {
        let tool = EnterPlanModeTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        assert!(result.content.contains("plan mode"));
    }

    #[tokio::test]
    async fn test_already_in_plan_mode() {
        let tool = EnterPlanModeTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        // Enter plan mode first
        ctx.app_state.write().await.tool_permission_context.mode = PermissionMode::Plan;
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 4: Register in mod.rs**

Add `pub mod enter_plan_mode_tool;` and `Box::new(enter_plan_mode_tool::EnterPlanModeTool)` to `get_builtin_tools()`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p atom-code plan_mode 2>&1 | tail -15`
Run: `cargo test -p atom-code enter_plan 2>&1 | tail -15`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add src/services/plan_mode/ src/services/mod.rs src/tools/enter_plan_mode_tool.rs src/tools/mod.rs
git commit -m "feat: add PlanModeService and EnterPlanModeTool"
```

---

### Task 3: ExitPlanModeTool

**Files:**
- Create: `src/tools/exit_plan_mode_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Implement ExitPlanModeTool**

Create `src/tools/exit_plan_mode_tool.rs`:

```rust
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
    fn name(&self) -> &str { "ExitPlanMode" }

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

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let session_id = ctx.app_state.read().await.session_id.clone();

        // Defensive: check plan mode
        {
            let state = ctx.app_state.read().await;
            if state.tool_permission_context.mode != PermissionMode::Plan {
                return Err(tool_err("Not in plan mode"));
            }
        }

        // Read plan file
        let plan = PlanModeService::read_plan(&session_id, None)
            .map_err(|e| tool_err(format!("Failed to read plan: {}", e)))?;
        let plan_path = PlanModeService::plan_file_path(&session_id, None);

        // Process allowedPrompts → add to always_allow_rules
        if let Some(prompts) = ctx.input.get("allowedPrompts").and_then(|v| v.as_array()) {
            let mut state = ctx.app_state.write().await;
            for prompt_obj in prompts {
                let tool_name = prompt_obj.get("tool").and_then(|v| v.as_str()).unwrap_or("Bash");
                let prompt = prompt_obj.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                state.tool_permission_context.always_allow_rules.push(PermissionRule {
                    tool_name: tool_name.to_string(),
                    rule_content: Some(prompt.to_string()),
                });
            }
        }

        // Exit plan mode (restore permission)
        {
            let mut state = ctx.app_state.write().await;
            PlanModeService::exit(&mut state)
                .map_err(|e| tool_err(format!("{}", e)))?;
        }

        // Check if Agent tool is available + swarms enabled
        let has_task_tool = is_agent_swarms_enabled()
            && ctx.tools.iter().any(|t| t.name() == "Agent");

        // Check if this is an agent context
        let is_agent = ctx.app_state.read().await.team_name.is_some();

        let response = serde_json::json!({
            "plan": plan,
            "isAgent": is_agent,
            "filePath": plan_path.display().to_string(),
            "hasTaskTool": has_task_tool,
            "awaitingLeaderApproval": false,
        });

        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
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

    fn is_read_only(&self) -> bool { false }
    fn should_defer(&self) -> bool { true }
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
        ctx.app_state.write().await.tool_permission_context.mode = PermissionMode::Plan;
        ctx.app_state.write().await.tool_permission_context.pre_plan_mode =
            Some(PermissionMode::Default);
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert!(parsed["plan"].is_null());
        assert_eq!(parsed["isAgent"], false);
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
        ctx.app_state.write().await.tool_permission_context.mode = PermissionMode::Plan;
        ctx.app_state.write().await.tool_permission_context.pre_plan_mode =
            Some(PermissionMode::Default);
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
    }
}
```

- [ ] **Step 2: Register in mod.rs**

Add `pub mod exit_plan_mode_tool;` and `Box::new(exit_plan_mode_tool::ExitPlanModeTool)` to `get_builtin_tools()`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code exit_plan 2>&1 | tail -15`
Expected: All pass.

- [ ] **Step 4: Commit**

```bash
git add src/tools/exit_plan_mode_tool.rs src/tools/mod.rs
git commit -m "feat: implement ExitPlanModeTool with allowedPrompts and plan reading"
```

---

### Task 4: Worktree Service + EnterWorktreeTool

**Files:**
- Create: `src/services/worktree/mod.rs`
- Modify: `src/services/mod.rs`
- Create: `src/tools/enter_worktree_tool.rs`
- Modify: `src/tools/mod.rs`
- Modify: `src/state/app_state.rs` (replace serde_json::Value with WorktreeSession)

- [ ] **Step 1: Create Worktree service**

Create `src/services/worktree/mod.rs`:

```rust
use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeSession {
    pub original_cwd: PathBuf,
    pub worktree_path: PathBuf,
    pub worktree_name: String,
    pub worktree_branch: Option<String>,
    pub original_branch: Option<String>,
    pub original_head_commit: Option<String>,
    pub session_id: String,
}

pub struct RemoveResult {
    pub discarded_files: u32,
    pub discarded_commits: u32,
}

pub struct WorktreeService;

impl WorktreeService {
    /// Validate slug: [a-zA-Z0-9._-], max 64, no .., no leading - or .
    pub fn validate_slug(slug: &str) -> Result<()> {
        if slug.is_empty() {
            bail!("Slug cannot be empty");
        }
        if slug.len() > 64 {
            bail!("Slug must be at most 64 characters");
        }
        if slug.starts_with('-') || slug.starts_with('.') {
            bail!("Slug cannot start with '-' or '.'");
        }
        if slug.contains("..") {
            bail!("Slug cannot contain '..' (path traversal)");
        }
        if !slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-') {
            bail!("Slug may only contain alphanumeric, '.', '_', '-'");
        }
        Ok(())
    }

    /// Find canonical git root from cwd
    pub async fn find_git_root(cwd: &Path) -> Result<PathBuf> {
        let output = Command::new("git")
            .args(["-C", &cwd.display().to_string(), "rev-parse", "--show-toplevel"])
            .output()
            .await
            .context("Failed to run git rev-parse")?;

        if !output.status.success() {
            bail!("Not in a git repository");
        }
        let root = String::from_utf8(output.stdout)?.trim().to_string();
        Ok(PathBuf::from(root))
    }

    /// Get current branch name
    async fn current_branch(cwd: &Path) -> Result<Option<String>> {
        let output = Command::new("git")
            .args(["-C", &cwd.display().to_string(), "branch", "--show-current"])
            .output()
            .await?;
        let branch = String::from_utf8(output.stdout)?.trim().to_string();
        Ok(if branch.is_empty() { None } else { Some(branch) })
    }

    /// Get HEAD commit hash
    async fn head_commit(cwd: &Path) -> Result<String> {
        let output = Command::new("git")
            .args(["-C", &cwd.display().to_string(), "rev-parse", "HEAD"])
            .output()
            .await?;
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    /// Create a git worktree
    pub async fn create(cwd: &Path, slug: &str, session_id: &str) -> Result<WorktreeSession> {
        Self::validate_slug(slug)?;

        let git_root = Self::find_git_root(cwd).await?;
        let worktree_path = git_root.join(".claude").join("worktrees").join(slug);
        let branch_name = format!("worktree-{}", slug);

        let original_branch = Self::current_branch(&git_root).await?;
        let original_head = Self::head_commit(&git_root).await?;

        // git worktree add -B branch path
        let output = Command::new("git")
            .args([
                "-C", &git_root.display().to_string(),
                "worktree", "add",
                &worktree_path.display().to_string(),
                "-B", &branch_name,
            ])
            .output()
            .await
            .context("Failed to run git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git worktree add failed: {}", stderr.trim());
        }

        Ok(WorktreeSession {
            original_cwd: cwd.to_path_buf(),
            worktree_path,
            worktree_name: slug.to_string(),
            worktree_branch: Some(branch_name),
            original_branch,
            original_head_commit: Some(original_head),
            session_id: session_id.to_string(),
        })
    }

    /// Remove a worktree and its branch
    pub async fn remove(session: &WorktreeSession) -> Result<RemoveResult> {
        let uncommitted = Self::count_uncommitted(&session.worktree_path).await.unwrap_or(0);
        let new_commits = if let Some(ref head) = session.original_head_commit {
            Self::count_new_commits(&session.worktree_path, head).await.unwrap_or(0)
        } else {
            0
        };

        // git worktree remove --force
        let output = Command::new("git")
            .args(["worktree", "remove", "--force", &session.worktree_path.display().to_string()])
            .output()
            .await
            .context("Failed to run git worktree remove")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git worktree remove failed: {}", stderr.trim());
        }

        // Delete branch
        if let Some(ref branch) = session.worktree_branch {
            let _ = Command::new("git")
                .args(["branch", "-D", branch])
                .output()
                .await;
        }

        Ok(RemoveResult {
            discarded_files: uncommitted,
            discarded_commits: new_commits,
        })
    }

    /// Count uncommitted files
    pub async fn count_uncommitted(worktree_path: &Path) -> Result<u32> {
        let output = Command::new("git")
            .args(["-C", &worktree_path.display().to_string(), "status", "--porcelain"])
            .output()
            .await?;
        let text = String::from_utf8(output.stdout)?;
        Ok(text.lines().filter(|l| !l.is_empty()).count() as u32)
    }

    /// Count new commits since original HEAD
    pub async fn count_new_commits(worktree_path: &Path, original_head: &str) -> Result<u32> {
        let range = format!("{}..HEAD", original_head);
        let output = Command::new("git")
            .args(["-C", &worktree_path.display().to_string(), "rev-list", &range, "--count"])
            .output()
            .await?;
        let text = String::from_utf8(output.stdout)?.trim().to_string();
        Ok(text.parse().unwrap_or(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_slug_valid() {
        assert!(WorktreeService::validate_slug("my-feature").is_ok());
        assert!(WorktreeService::validate_slug("test_123").is_ok());
        assert!(WorktreeService::validate_slug("v1.0.0").is_ok());
    }

    #[test]
    fn test_validate_slug_invalid() {
        assert!(WorktreeService::validate_slug("").is_err());
        assert!(WorktreeService::validate_slug("-starts-dash").is_err());
        assert!(WorktreeService::validate_slug(".hidden").is_err());
        assert!(WorktreeService::validate_slug("has/slash").is_err());
        assert!(WorktreeService::validate_slug("path..traversal").is_err());
        assert!(WorktreeService::validate_slug("has space").is_err());
        let long = "a".repeat(65);
        assert!(WorktreeService::validate_slug(&long).is_err());
    }
}
```

- [ ] **Step 2: Register worktree service**

In `src/services/mod.rs`, add:
```rust
pub mod worktree;
```

- [ ] **Step 3: Update AppState to use WorktreeSession type**

In `src/state/app_state.rs`, replace the `serde_json::Value` placeholder from Task 1:

```rust
pub current_worktree_session: Option<crate::services::worktree::WorktreeSession>,
```

- [ ] **Step 4: Implement EnterWorktreeTool**

Create `src/tools/enter_worktree_tool.rs`:

```rust
use async_trait::async_trait;

use crate::services::worktree::WorktreeService;
use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
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
    fn name(&self) -> &str { "EnterWorktree" }

    fn description(&self) -> &str {
        "Create an isolated git worktree and switch into it."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Slug for the worktree (alphanumeric, dots, underscores, dashes). Defaults to session ID prefix."
                }
            }
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        // Check not already in worktree
        {
            let state = ctx.app_state.read().await;
            if state.current_worktree_session.is_some() {
                return Err(tool_err("Already in a worktree session. Exit current worktree first."));
            }
        }

        let session_id = ctx.app_state.read().await.session_id.clone();
        let slug = ctx.input.get("name")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| session_id.chars().take(8).collect());

        let session = WorktreeService::create(&ctx.cwd, &slug, &session_id)
            .await
            .map_err(|e| tool_err(format!("{}", e)))?;

        let response = serde_json::json!({
            "worktreePath": session.worktree_path.display().to_string(),
            "worktreeBranch": session.worktree_branch,
            "message": format!("Created worktree '{}' at {}", slug, session.worktree_path.display()),
        });

        // Store session in app state
        ctx.app_state.write().await.current_worktree_session = Some(session);

        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool { false }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> {
        Some("create an isolated git worktree and switch into it")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(!EnterWorktreeTool.is_read_only());
    }

    #[tokio::test]
    async fn test_already_in_worktree() {
        let tool = EnterWorktreeTool;
        let ctx = ToolUseContext::test(serde_json::json!({ "name": "test" }));
        // Set worktree session
        ctx.app_state.write().await.current_worktree_session = Some(serde_json::from_value(
            serde_json::json!({
                "original_cwd": "/tmp",
                "worktree_path": "/tmp/wt",
                "worktree_name": "existing",
                "session_id": "test"
            })
        ).unwrap());
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 5: Register in mod.rs**

Add `pub mod enter_worktree_tool;` and `Box::new(enter_worktree_tool::EnterWorktreeTool)` to `get_builtin_tools()`.

- [ ] **Step 6: Run tests**

Run: `cargo test -p atom-code worktree 2>&1 | tail -15`
Run: `cargo test -p atom-code enter_worktree 2>&1 | tail -15`
Expected: All pass.

- [ ] **Step 7: Commit**

```bash
git add src/services/worktree/ src/services/mod.rs src/state/app_state.rs src/tools/enter_worktree_tool.rs src/tools/mod.rs
git commit -m "feat: add WorktreeService and EnterWorktreeTool"
```

---

### Task 5: ExitWorktreeTool

**Files:**
- Create: `src/tools/exit_worktree_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Implement ExitWorktreeTool**

Create `src/tools/exit_worktree_tool.rs`:

```rust
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
    fn name(&self) -> &str { "ExitWorktree" }

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
                    "description": "'keep' preserves the worktree, 'remove' deletes it"
                },
                "discard_changes": {
                    "type": "boolean",
                    "description": "Force remove even with uncommitted changes"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let action = ctx.input.get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'action' parameter"))?;

        if action != "keep" && action != "remove" {
            return Err(tool_err(format!("Invalid action '{}', must be 'keep' or 'remove'", action)));
        }

        let discard_changes = ctx.input.get("discard_changes")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let session = {
            let state = ctx.app_state.read().await;
            state.current_worktree_session.clone()
                .ok_or_else(|| tool_err("Not in a worktree session"))?
        };

        let mut discarded_files = 0u32;
        let mut discarded_commits = 0u32;

        if action == "remove" {
            // Safety gate: check for uncommitted changes
            let uncommitted = WorktreeService::count_uncommitted(&session.worktree_path)
                .await.unwrap_or(0);
            let new_commits = if let Some(ref head) = session.original_head_commit {
                WorktreeService::count_new_commits(&session.worktree_path, head)
                    .await.unwrap_or(0)
            } else {
                0
            };

            if (uncommitted > 0 || new_commits > 0) && !discard_changes {
                return Ok(ToolResult {
                    content: serde_json::json!({
                        "error": "Worktree has uncommitted changes or new commits",
                        "uncommittedFiles": uncommitted,
                        "newCommits": new_commits,
                        "hint": "Use discard_changes: true to force remove, or action: 'keep' to preserve"
                    }).to_string(),
                    is_error: true,
                    content_blocks: None,
                });
            }

            let result = WorktreeService::remove(&session)
                .await
                .map_err(|e| tool_err(format!("Failed to remove worktree: {}", e)))?;
            discarded_files = result.discarded_files;
            discarded_commits = result.discarded_commits;
        }

        // Clear worktree session
        ctx.app_state.write().await.current_worktree_session = None;

        let response = serde_json::json!({
            "action": action,
            "originalCwd": session.original_cwd.display().to_string(),
            "worktreePath": session.worktree_path.display().to_string(),
            "worktreeBranch": session.worktree_branch,
            "discardedFiles": discarded_files,
            "discardedCommits": discarded_commits,
            "message": format!(
                "Exited worktree '{}'. Action: {}.",
                session.worktree_name, action
            ),
        });

        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
        if let Ok(state) = ctx.app_state.try_read() {
            if state.current_worktree_session.is_none() {
                return ValidationResult::Disabled("Not in a worktree session".into());
            }
        }
        ValidationResult::Ok
    }

    fn is_read_only(&self) -> bool { false }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> {
        Some("exit a worktree session and return to the original directory")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(ExitWorktreeTool.name(), "ExitWorktree");
    }

    #[test]
    fn test_schema() {
        let schema = ExitWorktreeTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["action"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("action")));
    }

    #[test]
    fn test_should_defer() {
        assert!(ExitWorktreeTool.should_defer());
        assert!(!ExitWorktreeTool.is_read_only());
    }

    #[tokio::test]
    async fn test_not_in_worktree() {
        let tool = ExitWorktreeTool;
        let ctx = ToolUseContext::test(serde_json::json!({ "action": "keep" }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_action() {
        let tool = ExitWorktreeTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_action() {
        let tool = ExitWorktreeTool;
        let ctx = ToolUseContext::test(serde_json::json!({ "action": "invalid" }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Register in mod.rs**

Add `pub mod exit_worktree_tool;` and `Box::new(exit_worktree_tool::ExitWorktreeTool)` to `get_builtin_tools()`.

Update `test_deferred_tools` expected list to include all 4 new deferred tools:
```rust
let expected_deferred = &["SendMessage", "EnterPlanMode", "ExitPlanMode", "EnterWorktree", "ExitWorktree"];
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test -p atom-code 2>&1 | tail -30`
Expected: All pass.

- [ ] **Step 4: Commit**

```bash
git add src/tools/exit_worktree_tool.rs src/tools/mod.rs
git commit -m "feat: implement ExitWorktreeTool with safety gate and keep/remove actions"
```

---

### Task 6: Update Memory

- [ ] **Step 1: Update tool migration status**

Update memory file to reflect Batch 4 completion (28/54 tools).
