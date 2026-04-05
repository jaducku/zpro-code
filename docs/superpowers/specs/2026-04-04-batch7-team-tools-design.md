# Batch 7: Team Tools — Design Spec

**Date:** 2026-04-04
**Scope:** TeamCreateTool, TeamDeleteTool (2 tools + TeamService)

**Deferred:** LSPTool (experimental, opt-in only), PowerShellTool (Windows-only, complex security)

---

## 1. Team Service

**Location:** `src/services/team/mod.rs`

### Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub lead_agent_id: String,
    pub lead_session_id: Option<String>,
    #[serde(default)]
    pub members: Vec<TeamMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub agent_id: String,
    pub name: String,
    pub joined_at: DateTime<Utc>,
    #[serde(default)]
    pub is_active: bool,
}
```

### TeamService

```rust
pub struct TeamService;

impl TeamService {
    /// Create team: write config.json + create task list dir
    /// Lead agent ID: "team-lead@{team-name}"
    pub fn create(name: &str, description: Option<&str>, session_id: &str) -> Result<TeamConfig>;

    /// Delete team: remove team dir + task dir. Fails if active members exist.
    pub fn delete(name: &str) -> Result<()>;

    /// Load team config from disk
    pub fn load(name: &str) -> Result<Option<TeamConfig>>;

    /// Count active non-leader members
    pub fn active_member_count(config: &TeamConfig) -> usize;
}
```

### File paths

- Team config: `~/.atom/teams/{team-name}/config.json`
- Team tasks: `~/.atom/tasks/{team-name}/`

---

## 2. TeamCreateTool

**File:** `src/tools/team_create_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `team_name` | string | yes | Team identifier |
| `description` | string | no | Team description |

### Execution

1. Check `is_agent_swarms_enabled()`, error if not
2. Check `app_state.team_name` is None (can't lead multiple teams)
3. `TeamService::create(name, description, session_id)`
4. Set `app_state.team_name = Some(name)`
5. Return `{ team_name, team_file_path, lead_agent_id }`

### Properties

- `name() = "TeamCreate"`, `is_read_only() = false`, `should_defer() = true`
- `can_use()` → `is_agent_swarms_enabled()`
- `search_hint() = Some("create a multi-agent team for collaborative work")`

---

## 3. TeamDeleteTool

**File:** `src/tools/team_delete_tool.rs`

### Input

None — team name auto-detected from `app_state.team_name`.

### Execution

1. Get team name from `app_state.team_name`, error if None
2. Load team config, check active member count == 0
3. `TeamService::delete(name)`
4. Clear `app_state.team_name = None`
5. Clear team agents from `app_state.agent_name_registry`
6. Return `{ success: true, message, team_name }`

### Properties

- `name() = "TeamDelete"`, `is_read_only() = false`, `should_defer() = true`
- `can_use()` → `app_state.team_name.is_some()`
- `search_hint() = Some("disband a team and clean up resources")`

---

## 4. Implementation Order

1. TeamService + types
2. TeamCreateTool
3. TeamDeleteTool
4. Memory update
