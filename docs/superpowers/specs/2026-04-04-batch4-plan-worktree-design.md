# Batch 4: Plan & Worktree Tools — Design Spec

**Date:** 2026-04-04
**Scope:** EnterPlanModeTool, ExitPlanModeTool, EnterWorktreeTool, ExitWorktreeTool
**Approach:** Tool-First — each tool brings its infrastructure, services go in `src/services/`

---

## 1. AppState & Permission Changes

### PermissionMode::Plan

Verify `src/types/permissions.rs` has a `Plan` variant in `PermissionMode`. If not, add it.

### AppState additions

```rust
pub pre_plan_mode: Option<PermissionMode>,           // mode before plan mode entry
pub current_worktree_session: Option<WorktreeSession>, // active worktree session
```

---

## 2. Plan Mode Service

**Location:** `src/services/plan_mode/mod.rs`

```rust
pub struct PlanModeService;

impl PlanModeService {
    /// Enter plan mode: save current mode, switch to Plan
    pub fn enter(app_state: &mut AppState) -> Result<()> {
        app_state.pre_plan_mode = Some(app_state.tool_permission_context.mode.clone());
        app_state.tool_permission_context.mode = PermissionMode::Plan;
        Ok(())
    }

    /// Exit plan mode: restore previous mode
    pub fn exit(app_state: &mut AppState) -> Result<()> {
        if let Some(prev) = app_state.pre_plan_mode.take() {
            app_state.tool_permission_context.mode = prev;
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

    /// Read plan from file (None if file doesn't exist)
    pub fn read_plan(session_id: &str, agent_id: Option<&str>) -> Result<Option<String>> {
        let path = Self::plan_file_path(session_id, agent_id);
        if path.exists() {
            Ok(Some(std::fs::read_to_string(&path)?))
        } else {
            Ok(None)
        }
    }
}
```

---

## 3. EnterPlanModeTool

**File:** `src/tools/enter_plan_mode_tool.rs`

### Input

None — `{ "type": "object", "properties": {} }`

### Execution

1. Check not already in plan mode (`mode != Plan`), else error
2. `PlanModeService::enter(&mut app_state)`
3. Return `{ "message": "Entered plan mode. Design your approach before coding." }`

### Properties

- `name() = "EnterPlanMode"`
- `is_read_only() = true` (conceptually read-only — permission change is metadata)
- `should_defer() = true`
- `search_hint() = Some("switch to plan mode to design an approach before coding")`

---

## 4. ExitPlanModeTool

**File:** `src/tools/exit_plan_mode_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `allowedPrompts` | array of `{ tool: string, prompt: string }` | no | Bash commands to allow during plan execution |

### Execution

1. Check in plan mode (`mode == Plan`), else error
2. Read plan file via `PlanModeService::read_plan(session_id, None)`
3. If `allowedPrompts` provided, add to `always_allow_rules` as Bash permission rules
4. `PlanModeService::exit(&mut app_state)` — restore previous permission mode
5. Check if agent (team context) → if so, note `awaitingLeaderApproval = true`
6. Check if TaskCreate tool is available → `hasTaskTool`
7. Return output

### Output

```json
{
  "plan": "plan content or null",
  "isAgent": false,
  "filePath": "/path/to/plan.md",
  "hasTaskTool": true,
  "awaitingLeaderApproval": false
}
```

### Properties

- `name() = "ExitPlanMode"`
- `is_read_only() = false`
- `should_defer() = true`
- `search_hint() = Some("present plan for approval and start coding (plan mode only)")`
- `can_use()` → `app_state.tool_permission_context.mode == Plan` (via `try_read()`)

---

## 5. Worktree Service

**Location:** `src/services/worktree/mod.rs`

### WorktreeSession

```rust
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
```

### WorktreeService

```rust
pub struct WorktreeService;

impl WorktreeService {
    /// Validate slug: alphanumeric, dots, underscores, dashes; max 64 chars; no path traversal
    pub fn validate_slug(slug: &str) -> Result<()>;

    /// Create git worktree at .claude/worktrees/{slug}
    /// Branch: worktree-{slug} (slashes replaced with +)
    pub async fn create(cwd: &Path, slug: &str, session_id: &str) -> Result<WorktreeSession>;

    /// Remove worktree and delete branch
    pub async fn remove(session: &WorktreeSession) -> Result<RemoveResult>;

    /// Count uncommitted files via `git status --porcelain`
    pub async fn count_uncommitted(worktree_path: &Path) -> Result<u32>;

    /// Count new commits since original_head via `git rev-list`
    pub async fn count_new_commits(worktree_path: &Path, original_head: &str) -> Result<u32>;
}

pub struct RemoveResult {
    pub discarded_files: u32,
    pub discarded_commits: u32,
}
```

### Git commands

| Operation | Command |
|-----------|---------|
| Get current branch | `git -C {cwd} branch --show-current` |
| Get HEAD commit | `git -C {cwd} rev-parse HEAD` |
| Create worktree | `git -C {cwd} worktree add {path} -b {branch}` |
| Remove worktree | `git worktree remove {path}` |
| Delete branch | `git branch -D {branch}` |
| Uncommitted count | `git -C {path} status --porcelain` (count lines) |
| New commits count | `git -C {path} rev-list {head}..HEAD --count` |

All git commands via `tokio::process::Command`.

---

## 6. EnterWorktreeTool

**File:** `src/tools/enter_worktree_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | no | Slug for worktree. Default: first 8 chars of session_id |

### Execution

1. Check `current_worktree_session` is None, else error (already in worktree)
2. Derive slug from `name` or session_id prefix
3. `WorktreeService::validate_slug(slug)`
4. `WorktreeService::create(cwd, slug, session_id)`
5. `app_state.current_worktree_session = Some(session)`
6. Return `{ worktreePath, worktreeBranch, message }`

### Properties

- `name() = "EnterWorktree"`
- `is_read_only() = false`
- `should_defer() = true`
- `search_hint() = Some("create an isolated git worktree and switch into it")`

---

## 7. ExitWorktreeTool

**File:** `src/tools/exit_worktree_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `action` | string | yes | `"keep"` or `"remove"` |
| `discard_changes` | boolean | no | Force remove with uncommitted changes |

### Execution

1. Check `current_worktree_session` is Some, else error
2. If `action == "remove"`:
   a. `count_uncommitted()` + `count_new_commits()`
   b. If changes exist and `discard_changes != true` → error with change list
   c. `WorktreeService::remove(session)`
3. If `action == "keep"`:
   a. Worktree and branch preserved
4. Restore CWD to `session.original_cwd`
5. `app_state.current_worktree_session = None`
6. Return output

### Safety gate

When `action == "remove"` and there are uncommitted files or new commits, the tool refuses unless `discard_changes: true`. Error message includes counts.

### Properties

- `name() = "ExitWorktree"`
- `is_read_only() = false`
- `should_defer() = true`
- `search_hint() = Some("exit a worktree session and return to the original directory")`
- `can_use()` → `app_state.current_worktree_session.is_some()` (via `try_read()`)

---

## 8. Implementation Order

1. PermissionMode::Plan + AppState fields (`pre_plan_mode`, `current_worktree_session`)
2. Plan Mode service → EnterPlanModeTool
3. ExitPlanModeTool
4. Worktree service → EnterWorktreeTool
5. ExitWorktreeTool

---

## 9. New Dependencies

None — `tokio::process::Command` for git, `tokio::fs` for file I/O.

---

## 10. Known Limitations

- **Worktree hooks deferred:** TypeScript has `executeWorktreeCreateHook()` / `executeWorktreeRemoveHook()` for non-git VCS. Deferred to when hooks system is more mature.
- **Plan mode team approval simplified:** ExitPlanMode notes `awaitingLeaderApproval` but actual SendMessage dispatch to team lead deferred to when team system is active.
- **CWD mutation:** EnterWorktree changes the process CWD. This is session-wide and affects all subsequent tool calls. Same behavior as TypeScript original.
- **tmux session management omitted:** TypeScript manages tmux sessions for worktrees. Out of scope for CLI-only Rust implementation.
