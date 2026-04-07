# Batch 6: Schedule & Config Tools — Design Spec

**Date:** 2026-04-04
**Scope:** CronCreateTool, CronListTool, CronDeleteTool, ConfigTool (4 tools)
**Approach:** Tool-First

**Deferred:** RemoteTriggerTool — requires OAuth infrastructure. Will be separate batch.

---

## 1. Cron Service

**Location:** `src/services/cron/mod.rs`

### CronTask

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTask {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub recurring: bool,
    pub durable: bool,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_fired_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}
```

### CronStore

```rust
pub struct CronStore {
    session_tasks: Vec<CronTask>,   // in-memory only, lost on session end
    durable_path: PathBuf,          // ~/.atom/scheduled_tasks.json
}

impl CronStore {
    pub fn new() -> Self;  // durable_path = global_config_dir().join("scheduled_tasks.json")
    pub fn add(&mut self, task: CronTask) -> Result<()>;      // max 50 tasks
    pub fn remove(&mut self, id: &str) -> Result<()>;
    pub fn list(&self, agent_id: Option<&str>) -> Vec<&CronTask>;
    pub fn load_durable(&mut self) -> Result<()>;              // read from disk
    fn save_durable(&self) -> Result<()>;                      // write durable tasks to disk
}
```

- Session tasks: in-memory `Vec`, lost on exit
- Durable tasks: read/written to `~/.atom/scheduled_tasks.json` with fs2 locking
- Max 50 tasks total (session + durable)

### CronService

```rust
pub struct CronService;

impl CronService {
    /// Validate cron expression (5-field)
    pub fn validate_cron(expr: &str) -> Result<()>;

    /// Generate human-readable schedule description
    pub fn human_schedule(expr: &str) -> String;
}
```

Uses `cron` crate for parsing. Validation checks that expression matches at least one date
in the next year.

---

## 2. CronCreateTool

**File:** `src/tools/cron_create_tool.rs`

### Input

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `cron` | string | yes | — | 5-field cron expression |
| `prompt` | string | yes | — | Prompt to execute |
| `recurring` | boolean | no | true | Repeat on schedule |
| `durable` | boolean | no | false | Persist across sessions |

### Execution

1. Validate `cron` expression via `CronService::validate_cron()`
2. Generate `id` (uuid)
3. Create `CronTask` with fields
4. Add to `cron_store` (via AppState)
5. Return `{ id, humanSchedule, recurring, durable }`

### Properties

- `name() = "CronCreate"`
- `is_read_only() = false`
- `should_defer() = true`
- `search_hint() = Some("schedule a prompt to run on a cron schedule")`

---

## 3. CronListTool

**File:** `src/tools/cron_list_tool.rs`

### Input

None — `{ "type": "object", "properties": {} }`

### Execution

1. Get `cron_store` from AppState
2. `store.list(None)` — all tasks
3. Format as `{ jobs: [{ id, cron, humanSchedule, prompt, recurring, durable }] }`

### Properties

- `name() = "CronList"`
- `is_read_only() = true`
- `is_concurrent_safe() = true`
- `should_defer() = true`
- `search_hint() = Some("list all scheduled cron jobs")`

---

## 4. CronDeleteTool

**File:** `src/tools/cron_delete_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | string | yes | Cron job ID from CronCreate |

### Execution

1. Get `cron_store` from AppState
2. `store.remove(id)` — error if not found
3. Return `{ id }`

### Properties

- `name() = "CronDelete"`
- `is_read_only() = false`
- `should_defer() = true`
- `search_hint() = Some("cancel a scheduled cron job")`

---

## 5. ConfigTool

**File:** `src/tools/config_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `setting` | string | yes | Config key (e.g. `"theme"`, `"model"`, `"permissions.defaultMode"`) |
| `value` | any | no | Value to set. Omit to GET. |

### Settings Registry

```rust
pub struct SettingDef {
    pub key: &'static str,
    pub source: SettingSource,
    pub value_type: SettingType,
    pub description: &'static str,
}

pub enum SettingSource { Global, Project }
pub enum SettingType {
    String,
    Bool,
    Enum(&'static [&'static str]),
}
```

**Supported settings (v1, simplified):**

| Key | Source | Type | Description |
|-----|--------|------|-------------|
| `theme` | Global | Enum(`["dark","light","light-daltonized","dark-daltonized"]`) | Color theme |
| `verbose` | Global | Bool | Verbose output |
| `autoCompactEnabled` | Global | Bool | Auto-compact messages |
| `model` | Project | String | Model override |
| `permissions.defaultMode` | Project | Enum(`["default","plan","acceptEdits","bypassPermissions"]`) | Permission mode |
| `language` | Project | String | Response language |

Feature-gated settings (voice, remote, etc.) excluded from v1.

### GET execution

1. Look up `setting` in registry → get `SettingDef`
2. If `source == Global` → read from `AtomConfig::load()`, navigate dot-path
3. If `source == Project` → read from `AppState.settings`, navigate dot-path
4. Return `{ success: true, operation: "get", setting, value }`

### SET execution

1. Look up `setting` in registry
2. Validate `value` against `SettingType` (coerce `"true"/"false"` for Bool, check options for Enum)
3. If `source == Global` → update `AtomConfig` file
4. If `source == Project` → update `AppState.settings` (immediate)
5. Return `{ success: true, operation: "set", setting, previousValue, newValue }`

### Properties

- `name() = "ConfigTool"`
- `is_read_only() = false` (conservative — SET mutates)
- `should_defer() = true`
- `search_hint() = Some("get or set configuration settings")`

---

## 6. AppState Changes

```rust
#[serde(skip)]
pub cron_store: Option<crate::services::cron::CronStore>,
```

`#[serde(skip)]` — CronStore manages its own file persistence.
Initialized at session start, loaded durable tasks on init.

---

## 7. Implementation Order

1. `cron` crate dependency + CronService + CronStore
2. AppState.cron_store + CronCreateTool
3. CronListTool + CronDeleteTool
4. ConfigTool (settings registry, get/set)
5. Memory update

---

## 8. New Dependencies

| Crate | Purpose |
|-------|---------|
| `cron` | Cron expression parsing and next-fire calculation |

---

## 9. Known Limitations

- **RemoteTriggerTool deferred** — requires OAuth token management. Separate batch.
- **Cron scheduler loop not implemented** — only CRUD. The actual scheduler polling loop
  (fires when REPL idle) is a future enhancement; tools just manage the task registry.
- **ConfigTool settings limited** — v1 has ~6 core settings. Voice, remote, teammate settings
  excluded until those features exist.
- **Jitter and auto-expiry deferred** — cron tasks are stored but firing logic is separate.
- **No cron validation against next-year window** — validates syntax only, not whether it
  matches any date. Can be enhanced later with `cron` crate's iterator.
