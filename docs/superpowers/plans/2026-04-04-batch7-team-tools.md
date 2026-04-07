# Batch 7: Team Tools — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add TeamCreateTool and TeamDeleteTool with a TeamService for file-based team management.

**Architecture:** Tool-first. TeamService manages team config files at `~/.atom/teams/{name}/config.json`. Follows existing service patterns (TaskListStore, Mailbox).

**Tech Stack:** Rust, async-trait, tokio, serde, chrono

**Spec:** `docs/superpowers/specs/2026-04-04-batch7-team-tools-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/services/team/mod.rs` | Create | TeamService, TeamConfig, TeamMember |
| `src/services/mod.rs` | Modify | `pub mod team;` |
| `src/tools/team_create_tool.rs` | Create | TeamCreateTool |
| `src/tools/team_delete_tool.rs` | Create | TeamDeleteTool |
| `src/tools/mod.rs` | Modify | Register 2 tools |

---

### Task 1: TeamService

**Files:** Create `src/services/team/mod.rs`, modify `src/services/mod.rs`

Full implementation with types, CRUD, and tests. See spec Section 1 for types.

Key methods:
- `create(name, desc, session_id)` → write config.json, create tasks dir, return TeamConfig
- `delete(name)` → check no active members, remove dirs
- `load(name)` → read config.json
- `active_member_count(config)` → count members where is_active && agent_id != lead

File paths: `global_config_dir().join("teams/{name}/config.json")` and `global_config_dir().join("tasks/{name}/")`.

Tests: create/load roundtrip, delete, delete with active members (error), active_member_count.

Commit: `"feat: add TeamService with team config management"`

---

### Task 2: TeamCreateTool + TeamDeleteTool

**Files:** Create both tool files, modify `src/tools/mod.rs`

**TeamCreateTool:**
- Input: `{ team_name (required), description (optional) }`
- Execute: check swarms enabled, check not already leading, create team, set app_state.team_name
- Properties: should_defer=true, can_use checks is_agent_swarms_enabled()

**TeamDeleteTool:**
- Input: `{}` (auto-detect team from app_state)
- Execute: check team_name set, load config, check no active members, delete, clear state
- Properties: should_defer=true, can_use checks team_name.is_some()

Tests for both: name, schema, properties, key execute paths.

Register both + update test_deferred_tools.

Commit: `"feat: implement TeamCreateTool and TeamDeleteTool"`

---

### Task 3: Update Memory

Update tool migration status to 36/54.
