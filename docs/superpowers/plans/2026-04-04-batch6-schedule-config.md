# Batch 6: Schedule & Config Tools — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 4 tools (CronCreate, CronList, CronDelete, ConfigTool) with a cron service and settings registry.

**Architecture:** Tool-first. CronStore manages in-memory + durable tasks with fs2 locking. ConfigTool uses a static settings registry mapping keys to Global/Project sources. `cron` crate 0.16 for expression parsing.

**Tech Stack:** Rust, async-trait, tokio, cron 0.16, serde, fs2

**Spec:** `docs/superpowers/specs/2026-04-04-batch6-schedule-config-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `Cargo.toml` | Modify | Add `cron = "0.16"` |
| `src/services/cron/mod.rs` | Create | CronTask, CronStore, CronService |
| `src/services/mod.rs` | Modify | `pub mod cron;` |
| `src/state/app_state.rs` | Modify | Add `cron_store` field |
| `src/tools/cron_create_tool.rs` | Create | CronCreateTool |
| `src/tools/cron_list_tool.rs` | Create | CronListTool |
| `src/tools/cron_delete_tool.rs` | Create | CronDeleteTool |
| `src/tools/config_tool.rs` | Create | ConfigTool with settings registry |
| `src/tools/mod.rs` | Modify | Register 4 tools |

---

### Task 1: Cron Service + CronStore + AppState

**Files:**
- Modify: `Cargo.toml` (add `cron = "0.16"`)
- Create: `src/services/cron/mod.rs`
- Modify: `src/services/mod.rs`
- Modify: `src/state/app_state.rs`

- [ ] **Step 1: Add cron dependency**

In `Cargo.toml` `[dependencies]`, add:
```toml
cron = "0.16"
```

- [ ] **Step 2: Create cron service**

Create `src/services/cron/mod.rs`:

```rust
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};

const MAX_TASKS: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTask {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    #[serde(default = "default_true")]
    pub recurring: bool,
    #[serde(default)]
    pub durable: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

fn default_true() -> bool { true }

pub struct CronStore {
    session_tasks: Vec<CronTask>,
    durable_path: PathBuf,
}

impl CronStore {
    pub fn new() -> Self {
        Self {
            session_tasks: Vec::new(),
            durable_path: crate::utils::paths::global_config_dir()
                .join("scheduled_tasks.json"),
        }
    }

    /// Total count of all tasks (session + durable)
    fn total_count(&self) -> usize {
        self.session_tasks.len() + self.load_durable_tasks().len()
    }

    /// Add a task (enforces max 50 limit)
    pub fn add(&mut self, task: CronTask) -> Result<()> {
        if self.total_count() >= MAX_TASKS {
            bail!("Maximum of {} scheduled tasks reached", MAX_TASKS);
        }
        if task.durable {
            let mut durable = self.load_durable_tasks();
            durable.push(task);
            self.save_durable_tasks(&durable)?;
        } else {
            self.session_tasks.push(task);
        }
        Ok(())
    }

    /// Remove a task by id (searches both session and durable)
    pub fn remove(&mut self, id: &str) -> Result<()> {
        // Try session tasks first
        let before = self.session_tasks.len();
        self.session_tasks.retain(|t| t.id != id);
        if self.session_tasks.len() < before {
            return Ok(());
        }
        // Try durable tasks
        let mut durable = self.load_durable_tasks();
        let before = durable.len();
        durable.retain(|t| t.id != id);
        if durable.len() < before {
            self.save_durable_tasks(&durable)?;
            return Ok(());
        }
        bail!("Cron task '{}' not found", id)
    }

    /// List all tasks, optionally filtered by agent_id
    pub fn list(&self, agent_id: Option<&str>) -> Vec<CronTask> {
        let durable = self.load_durable_tasks();
        let all = self.session_tasks.iter().chain(durable.iter());
        match agent_id {
            Some(aid) => all.filter(|t| t.agent_id.as_deref() == Some(aid)).cloned().collect(),
            None => all.cloned().collect(),
        }
    }

    fn load_durable_tasks(&self) -> Vec<CronTask> {
        if !self.durable_path.exists() {
            return Vec::new();
        }
        match std::fs::read_to_string(&self.durable_path) {
            Ok(content) => {
                #[derive(Deserialize)]
                struct DurableFile { tasks: Vec<CronTask> }
                serde_json::from_str::<DurableFile>(&content)
                    .map(|f| f.tasks)
                    .unwrap_or_default()
            }
            Err(_) => Vec::new(),
        }
    }

    fn save_durable_tasks(&self, tasks: &[CronTask]) -> Result<()> {
        if let Some(parent) = self.durable_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(&serde_json::json!({ "tasks": tasks }))?;
        std::fs::write(&self.durable_path, content)?;
        Ok(())
    }
}

pub struct CronService;

impl CronService {
    /// Validate a 5-field cron expression
    pub fn validate_cron(expr: &str) -> Result<()> {
        // cron crate requires 6 or 7 fields (with seconds). Prepend "0 " for 5-field.
        let with_seconds = format!("0 {}", expr);
        Schedule::from_str(&with_seconds)
            .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", expr, e))?;
        Ok(())
    }

    /// Generate a human-readable schedule description
    pub fn human_schedule(expr: &str) -> String {
        // Simple field-based description
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return format!("cron: {}", expr);
        }
        let (min, hour, dom, mon, dow) = (parts[0], parts[1], parts[2], parts[3], parts[4]);
        if min == "*" && hour == "*" {
            return "Every minute".to_string();
        }
        if hour == "*" {
            return format!("Every hour at minute {}", min);
        }
        if dom == "*" && mon == "*" && dow == "*" {
            return format!("Daily at {}:{}", hour, if min.len() == 1 { format!("0{}", min) } else { min.to_string() });
        }
        format!("cron: {}", expr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_store(dir: &TempDir) -> CronStore {
        CronStore {
            session_tasks: Vec::new(),
            durable_path: dir.path().join("scheduled_tasks.json"),
        }
    }

    fn make_task(id: &str, durable: bool) -> CronTask {
        CronTask {
            id: id.to_string(),
            cron: "0 9 * * *".to_string(),
            prompt: "test prompt".to_string(),
            recurring: true,
            durable,
            created_at: Utc::now(),
            last_fired_at: None,
            agent_id: None,
        }
    }

    #[test]
    fn test_validate_cron_valid() {
        assert!(CronService::validate_cron("0 9 * * *").is_ok());
        assert!(CronService::validate_cron("*/5 * * * *").is_ok());
        assert!(CronService::validate_cron("0 0 1 1 *").is_ok());
    }

    #[test]
    fn test_validate_cron_invalid() {
        assert!(CronService::validate_cron("invalid").is_err());
        assert!(CronService::validate_cron("60 * * * *").is_err());
        assert!(CronService::validate_cron("").is_err());
    }

    #[test]
    fn test_human_schedule() {
        assert_eq!(CronService::human_schedule("* * * * *"), "Every minute");
        assert_eq!(CronService::human_schedule("30 * * * *"), "Every hour at minute 30");
        assert_eq!(CronService::human_schedule("0 9 * * *"), "Daily at 9:00");
    }

    #[test]
    fn test_add_session_task() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);
        store.add(make_task("t1", false)).unwrap();
        assert_eq!(store.list(None).len(), 1);
    }

    #[test]
    fn test_add_durable_task() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);
        store.add(make_task("t1", true)).unwrap();
        assert_eq!(store.list(None).len(), 1);
        assert!(store.durable_path.exists());
    }

    #[test]
    fn test_remove_task() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);
        store.add(make_task("t1", false)).unwrap();
        store.remove("t1").unwrap();
        assert!(store.list(None).is_empty());
    }

    #[test]
    fn test_remove_nonexistent() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);
        assert!(store.remove("nonexistent").is_err());
    }

    #[test]
    fn test_max_tasks() {
        let dir = TempDir::new().unwrap();
        let mut store = test_store(&dir);
        for i in 0..50 {
            store.add(make_task(&format!("t{}", i), false)).unwrap();
        }
        assert!(store.add(make_task("t50", false)).is_err());
    }
}
```

- [ ] **Step 3: Register cron service**

In `src/services/mod.rs`, add: `pub mod cron;`

- [ ] **Step 4: Add cron_store to AppState**

In `src/state/app_state.rs`, add field after `current_worktree_session`:

```rust
    /// Cron task store — manages its own file persistence
    #[serde(skip)]
    pub cron_store: Option<crate::services::cron::CronStore>,
```

Add `cron_store: None,` to Default impl.

- [ ] **Step 5: Run tests**

Run: `cargo test -p atom-code cron 2>&1 | tail -15`
Expected: All pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/services/cron/ src/services/mod.rs src/state/app_state.rs
git commit -m "feat: add CronService, CronStore with session and durable task support"
```

---

### Task 2: CronCreateTool

**Files:**
- Create: `src/tools/cron_create_tool.rs` (NOTE: this is a NEW tool, different from the existing `task_create_tool.rs`)
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Implement CronCreateTool**

Create `src/tools/cron_create_tool.rs`:

```rust
use async_trait::async_trait;

use crate::services::cron::{CronService, CronTask};
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct CronCreateTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "CronCreate".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for CronCreateTool {
    fn name(&self) -> &str { "CronCreate" }

    fn description(&self) -> &str {
        "Schedule a prompt to run on a cron schedule."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "cron": {
                    "type": "string",
                    "description": "5-field cron expression (e.g. '0 9 * * *' for daily at 9am)"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to execute on schedule"
                },
                "recurring": {
                    "type": "boolean",
                    "description": "Whether to repeat (default: true)"
                },
                "durable": {
                    "type": "boolean",
                    "description": "Persist across sessions (default: false)"
                }
            },
            "required": ["cron", "prompt"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let cron_expr = ctx.input.get("cron")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'cron' parameter"))?;

        let prompt = ctx.input.get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'prompt' parameter"))?;

        let recurring = ctx.input.get("recurring")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let durable = ctx.input.get("durable")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Validate cron expression
        CronService::validate_cron(cron_expr)
            .map_err(|e| tool_err(format!("{}", e)))?;

        let id = uuid::Uuid::new_v4().to_string();
        let human = CronService::human_schedule(cron_expr);

        let task = CronTask {
            id: id.clone(),
            cron: cron_expr.to_string(),
            prompt: prompt.to_string(),
            recurring,
            durable,
            created_at: chrono::Utc::now(),
            last_fired_at: None,
            agent_id: None,
        };

        // Add to store
        {
            let mut state = ctx.app_state.write().await;
            let store = state.cron_store.get_or_insert_with(
                crate::services::cron::CronStore::new
            );
            store.add(task).map_err(|e| tool_err(format!("{}", e)))?;
        }

        let response = serde_json::json!({
            "id": id,
            "humanSchedule": human,
            "recurring": recurring,
            "durable": durable,
        });

        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool { false }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> {
        Some("schedule a prompt to run on a cron schedule")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(CronCreateTool.name(), "CronCreate");
    }

    #[test]
    fn test_schema() {
        let schema = CronCreateTool.input_schema();
        assert_eq!(schema["type"], "object");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("cron")));
        assert!(required.contains(&serde_json::json!("prompt")));
    }

    #[test]
    fn test_should_defer() {
        assert!(CronCreateTool.should_defer());
        assert!(!CronCreateTool.is_read_only());
    }

    #[tokio::test]
    async fn test_create_cron() {
        let tool = CronCreateTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "cron": "0 9 * * *",
            "prompt": "check status"
        }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert!(parsed["id"].is_string());
        assert_eq!(parsed["recurring"], true);
        assert_eq!(parsed["durable"], false);
    }

    #[tokio::test]
    async fn test_invalid_cron() {
        let tool = CronCreateTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "cron": "invalid",
            "prompt": "test"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_cron() {
        let tool = CronCreateTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "prompt": "test"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Register**

Add `pub mod cron_create_tool;` and `Box::new(cron_create_tool::CronCreateTool)` to `get_builtin_tools()`.
Update `test_deferred_tools` expected list.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code cron_create 2>&1 | tail -15`

- [ ] **Step 4: Commit**

```bash
git add src/tools/cron_create_tool.rs src/tools/mod.rs
git commit -m "feat: implement CronCreateTool"
```

---

### Task 3: CronListTool + CronDeleteTool

**Files:**
- Create: `src/tools/cron_list_tool.rs`
- Create: `src/tools/cron_delete_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Implement CronListTool**

Create `src/tools/cron_list_tool.rs`:

```rust
use async_trait::async_trait;

use crate::services::cron::CronService;
use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct CronListTool;

#[async_trait]
impl Tool for CronListTool {
    fn name(&self) -> &str { "CronList" }
    fn description(&self) -> &str { "List all scheduled cron jobs." }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let state = ctx.app_state.read().await;
        let tasks = match &state.cron_store {
            Some(store) => store.list(None),
            None => Vec::new(),
        };

        let jobs: Vec<serde_json::Value> = tasks.iter().map(|t| {
            serde_json::json!({
                "id": t.id,
                "cron": t.cron,
                "humanSchedule": CronService::human_schedule(&t.cron),
                "prompt": t.prompt,
                "recurring": t.recurring,
                "durable": t.durable,
            })
        }).collect();

        let response = serde_json::json!({ "jobs": jobs });
        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> { Some("list all scheduled cron jobs") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() { assert_eq!(CronListTool.name(), "CronList"); }

    #[test]
    fn test_properties() {
        assert!(CronListTool.is_read_only());
        assert!(CronListTool.is_concurrent_safe());
        assert!(CronListTool.should_defer());
    }

    #[tokio::test]
    async fn test_list_empty() {
        let tool = CronListTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["jobs"].as_array().unwrap().len(), 0);
    }
}
```

- [ ] **Step 2: Implement CronDeleteTool**

Create `src/tools/cron_delete_tool.rs`:

```rust
use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct CronDeleteTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool { tool_name: "CronDelete".into(), message: msg.into() }
}

#[async_trait]
impl Tool for CronDeleteTool {
    fn name(&self) -> &str { "CronDelete" }
    fn description(&self) -> &str { "Cancel a scheduled cron job by ID." }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Cron job ID from CronCreate" }
            },
            "required": ["id"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let id = ctx.input.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'id' parameter"))?;

        {
            let mut state = ctx.app_state.write().await;
            let store = state.cron_store.get_or_insert_with(
                crate::services::cron::CronStore::new
            );
            store.remove(id).map_err(|e| tool_err(format!("{}", e)))?;
        }

        let response = serde_json::json!({ "id": id });
        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool { false }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> { Some("cancel a scheduled cron job") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() { assert_eq!(CronDeleteTool.name(), "CronDelete"); }

    #[test]
    fn test_schema() {
        let schema = CronDeleteTool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("id")));
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let tool = CronDeleteTool;
        let ctx = ToolUseContext::test(serde_json::json!({ "id": "nonexistent" }));
        // cron_store is None, get_or_insert creates empty, then remove fails
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_id() {
        let tool = CronDeleteTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Register both**

Add `pub mod cron_list_tool;`, `pub mod cron_delete_tool;` and both to `get_builtin_tools()`.
Update `test_deferred_tools` expected list.

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code cron 2>&1 | tail -20`

- [ ] **Step 5: Commit**

```bash
git add src/tools/cron_list_tool.rs src/tools/cron_delete_tool.rs src/tools/mod.rs
git commit -m "feat: implement CronListTool and CronDeleteTool"
```

---

### Task 4: ConfigTool

**Files:**
- Create: `src/tools/config_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Implement ConfigTool with settings registry**

Create `src/tools/config_tool.rs`:

```rust
use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct ConfigTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool { tool_name: "ConfigTool".into(), message: msg.into() }
}

// --- Settings Registry ---

#[derive(Debug, Clone, Copy)]
enum SettingSource { Global, Project }

#[derive(Debug, Clone)]
enum SettingType {
    String,
    Bool,
    Enum(&'static [&'static str]),
}

struct SettingDef {
    key: &'static str,
    source: SettingSource,
    value_type: SettingType,
    description: &'static str,
}

const SETTINGS: &[SettingDef] = &[
    SettingDef { key: "theme", source: SettingSource::Global, value_type: SettingType::Enum(&["dark", "light", "light-daltonized", "dark-daltonized"]), description: "Color theme" },
    SettingDef { key: "verbose", source: SettingSource::Global, value_type: SettingType::Bool, description: "Verbose output" },
    SettingDef { key: "autoCompactEnabled", source: SettingSource::Global, value_type: SettingType::Bool, description: "Auto-compact messages when context is large" },
    SettingDef { key: "model", source: SettingSource::Project, value_type: SettingType::String, description: "Model override for this project" },
    SettingDef { key: "permissions.defaultMode", source: SettingSource::Project, value_type: SettingType::Enum(&["default", "plan", "acceptEdits", "bypassPermissions"]), description: "Default permission mode" },
    SettingDef { key: "language", source: SettingSource::Project, value_type: SettingType::String, description: "Response language" },
];

fn find_setting(key: &str) -> Option<&'static SettingDef> {
    SETTINGS.iter().find(|s| s.key == key)
}

fn validate_value(def: &SettingDef, value: &serde_json::Value) -> Result<serde_json::Value, String> {
    match &def.value_type {
        SettingType::String => {
            if value.is_string() { Ok(value.clone()) }
            else { Ok(serde_json::Value::String(value.to_string())) }
        }
        SettingType::Bool => {
            if let Some(b) = value.as_bool() {
                Ok(serde_json::Value::Bool(b))
            } else if let Some(s) = value.as_str() {
                match s {
                    "true" => Ok(serde_json::Value::Bool(true)),
                    "false" => Ok(serde_json::Value::Bool(false)),
                    _ => Err(format!("Expected boolean for '{}', got '{}'", def.key, s)),
                }
            } else {
                Err(format!("Expected boolean for '{}'", def.key))
            }
        }
        SettingType::Enum(options) => {
            let s = value.as_str()
                .ok_or_else(|| format!("Expected string for '{}', got {}", def.key, value))?;
            if options.contains(&s) {
                Ok(serde_json::Value::String(s.to_string()))
            } else {
                Err(format!("Invalid value '{}' for '{}'. Options: {:?}", s, def.key, options))
            }
        }
    }
}

/// Navigate a dot-path in a JSON value (e.g. "permissions.defaultMode")
fn get_nested(value: &serde_json::Value, path: &str) -> Option<serde_json::Value> {
    let mut current = value;
    for key in path.split('.') {
        current = current.get(key)?;
    }
    Some(current.clone())
}

/// Set a value at a dot-path in a JSON value
fn set_nested(value: &mut serde_json::Value, path: &str, new_val: serde_json::Value) {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;
    for (i, key) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            current[key] = new_val.clone();
            return;
        }
        if !current.get(key).map(|v| v.is_object()).unwrap_or(false) {
            current[key] = serde_json::json!({});
        }
        current = current.get_mut(key).unwrap();
    }
}

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &str { "ConfigTool" }
    fn description(&self) -> &str { "Get or set configuration settings." }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "setting": {
                    "type": "string",
                    "description": "Config key (e.g. 'theme', 'model', 'permissions.defaultMode')"
                },
                "value": {
                    "description": "Value to set. Omit to GET current value."
                }
            },
            "required": ["setting"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let setting = ctx.input.get("setting")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'setting' parameter"))?;

        let def = find_setting(setting)
            .ok_or_else(|| tool_err(format!(
                "Unknown setting '{}'. Available: {}",
                setting,
                SETTINGS.iter().map(|s| s.key).collect::<Vec<_>>().join(", ")
            )))?;

        let value = ctx.input.get("value");

        if value.is_none() || value == Some(&serde_json::Value::Null) {
            // GET
            let current = match def.source {
                SettingSource::Global => {
                    let config = crate::services::config::AtomConfig::load();
                    let json = serde_json::to_value(&config).unwrap_or_default();
                    get_nested(&json, setting)
                }
                SettingSource::Project => {
                    let state = ctx.app_state.read().await;
                    let json = serde_json::to_value(&state.settings).unwrap_or_default();
                    get_nested(&json, setting)
                }
            };

            let response = serde_json::json!({
                "success": true,
                "operation": "get",
                "setting": setting,
                "value": current,
            });
            return Ok(ToolResult {
                content: response.to_string(),
                is_error: false,
                content_blocks: None,
            });
        }

        // SET
        let raw_value = value.unwrap();
        let validated = validate_value(def, raw_value)
            .map_err(|e| tool_err(e))?;

        let previous = match def.source {
            SettingSource::Global => {
                let mut config = crate::services::config::AtomConfig::load();
                let mut json = serde_json::to_value(&config).unwrap_or_default();
                let prev = get_nested(&json, setting);
                set_nested(&mut json, setting, validated.clone());
                config = serde_json::from_value(json).unwrap_or_default();
                config.save().map_err(|e| tool_err(format!("Failed to save config: {}", e)))?;
                prev
            }
            SettingSource::Project => {
                let mut state = ctx.app_state.write().await;
                let mut json = serde_json::to_value(&state.settings).unwrap_or_default();
                let prev = get_nested(&json, setting);
                set_nested(&mut json, setting, validated.clone());
                state.settings = serde_json::from_value(json).unwrap_or_default();
                prev
            }
        };

        let response = serde_json::json!({
            "success": true,
            "operation": "set",
            "setting": setting,
            "previousValue": previous,
            "newValue": validated,
        });
        Ok(ToolResult {
            content: response.to_string(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool { false }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> { Some("get or set configuration settings") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() { assert_eq!(ConfigTool.name(), "ConfigTool"); }

    #[test]
    fn test_schema() {
        let schema = ConfigTool.input_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("setting")));
    }

    #[test]
    fn test_find_setting() {
        assert!(find_setting("theme").is_some());
        assert!(find_setting("model").is_some());
        assert!(find_setting("nonexistent").is_none());
    }

    #[test]
    fn test_validate_bool_coercion() {
        let def = find_setting("verbose").unwrap();
        assert_eq!(
            validate_value(def, &serde_json::json!("true")).unwrap(),
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            validate_value(def, &serde_json::json!(false)).unwrap(),
            serde_json::Value::Bool(false)
        );
    }

    #[test]
    fn test_validate_enum() {
        let def = find_setting("theme").unwrap();
        assert!(validate_value(def, &serde_json::json!("dark")).is_ok());
        assert!(validate_value(def, &serde_json::json!("invalid")).is_err());
    }

    #[test]
    fn test_get_nested() {
        let json = serde_json::json!({ "a": { "b": "hello" } });
        assert_eq!(get_nested(&json, "a.b"), Some(serde_json::json!("hello")));
        assert_eq!(get_nested(&json, "a.c"), None);
    }

    #[test]
    fn test_set_nested() {
        let mut json = serde_json::json!({});
        set_nested(&mut json, "a.b", serde_json::json!("hello"));
        assert_eq!(json["a"]["b"], "hello");
    }

    #[tokio::test]
    async fn test_unknown_setting() {
        let tool = ConfigTool;
        let ctx = ToolUseContext::test(serde_json::json!({ "setting": "nonexistent" }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_setting() {
        let tool = ConfigTool;
        let ctx = ToolUseContext::test(serde_json::json!({ "setting": "verbose" }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["operation"], "get");
    }

    #[tokio::test]
    async fn test_missing_setting() {
        let tool = ConfigTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Register**

Add `pub mod config_tool;` and `Box::new(config_tool::ConfigTool)` to `get_builtin_tools()`.
Update `test_deferred_tools` expected list.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code config_tool 2>&1 | tail -15`
Run: `cargo test -p atom-code 2>&1 | tail -5` (full suite)

- [ ] **Step 4: Commit**

```bash
git add src/tools/config_tool.rs src/tools/mod.rs
git commit -m "feat: implement ConfigTool with settings registry and get/set"
```

---

### Task 5: Update Memory

- [ ] **Step 1: Update tool migration status to 34/54**
