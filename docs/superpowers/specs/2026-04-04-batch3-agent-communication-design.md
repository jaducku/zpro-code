# Batch 3: Agent Communication Tools — Design Spec

**Date:** 2026-04-04
**Scope:** ToolSearchTool, SendMessageTool, SyntheticOutputTool, BriefTool (SendUserMessage)
**Approach:** Tool-First — each tool brings its infrastructure, services go in `src/services/`

---

## 1. Tool Trait Extension (Deferred Tool Support)

### New methods on `Tool` trait (`src/tool.rs`)

```rust
fn should_defer(&self) -> bool { false }
fn always_load(&self) -> bool { false }
fn search_hint(&self) -> Option<&str> { None }
```

All have default implementations — existing tools unchanged.

### Deferred tool determination

```rust
pub fn is_deferred_tool(tool: &dyn Tool) -> bool {
    if tool.always_load() { return false; }
    if tool.is_mcp() { return true; }
    if tool.name() == "ToolSearch" { return false; }
    tool.should_defer()
}
```

Added as a free function in `src/tools/mod.rs`.

### ToolDefinition change

```rust
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "is_false")]
    pub defer_loading: bool,  // NEW — signals API to omit full schema
}

fn is_false(v: &bool) -> bool { !v }
```

`From<&dyn Tool>` sets `defer_loading` based on `is_deferred_tool()`.
`defer_loading: false` is omitted from serialization to keep the wire format clean.

### ToolResultBlock extension for tool_reference

ToolSearchTool returns `tool_reference` content blocks so the API can inject tool schemas.

**Step 1:** Add `ToolReference` variant to the **existing** `crate::types::content::ContentBlock` enum
(in `src/types/content.rs`), not a new type:

```rust
// In existing ContentBlock enum, add:
#[serde(rename = "tool_reference")]
ToolReference { tool_name: String },
```

**Step 2:** Extend `ToolResultBlock` to support structured content:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: ToolResultContent,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<crate::types::content::ContentBlock>),
}
```

**Step 3:** Update query engine (`src/query/engine.rs`) where `ToolResultBlock` is converted
to API message content. The conversion must propagate `ToolResultContent::Blocks` through to
the API, not wrap it in `Text`. Specifically, where the engine calls `b.content.clone()` to
build `ContentBlock::ToolResult`, it must match on the enum and handle both variants.

This is backward-compatible: existing tools return `ToolResultContent::Text(string)`,
ToolSearchTool returns `ToolResultContent::Blocks(vec![ContentBlock::ToolReference { ... }])`.

---

## 2. ToolSearchTool

**File:** `src/tools/tool_search_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `query` | string | yes | `"select:Read,Edit"`, `"+slack send"`, or keyword search |
| `max_results` | number | no | Default 5 |

### Query modes

1. **`select:Name1,Name2`** — exact name match from `ctx.tools` (deferred or not)
2. **Exact name fast path** — if query exactly matches a tool name (case-insensitive), return it immediately
3. **`mcp__` prefix match** — if query starts with `mcp__`, find tools by prefix
4. **`+required optional`** — `+` prefix terms must all match, rest ranked by score
5. **keyword search** — all terms optional, ranked by score

### Scoring

Modes 1-3 (select, exact-name, mcp-prefix) match **all tools** regardless of defer status.
Modes 4-5 (+required, keyword) search **deferred tools only**.

| Match location | Full word | Partial |
|----------------|-----------|---------|
| name (MCP) | +12 | +6 |
| name (regular) | +10 | +5 |
| search_hint | +4 | — |
| description | +2 | — |

### Output

Returns `tool_reference` content blocks via `ToolResultContent::Blocks`:

```rust
ToolResult {
    content: ToolResultContent::Blocks(
        matched_names.iter().map(|name| ContentBlock::ToolReference {
            tool_name: name.clone(),
        }).collect()
    ),
    is_error: false,
}
```

The API expands `tool_reference` blocks into full tool schemas server-side.

Additionally includes metadata as a text block:
```json
{ "query": "...", "total_deferred_tools": 12, "matches": ["Read", "Edit"] }
```

### search_hint values for new tools

| Tool | search_hint |
|------|-------------|
| SendMessageTool | `"send messages to agent teammates (swarm protocol)"` |
| SyntheticOutputTool | `"return the final response as structured JSON"` |
| BriefTool | `"send a visible message to the user"` |

### Properties

- `name() = "ToolSearch"`
- `is_read_only() = true`
- `is_concurrent_safe() = true`
- `always_load() = true`

---

## 3. SendMessageTool

**File:** `src/tools/send_message_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `to` | string | yes | Agent name, `"*"`, `"uds:<path>"`, `"bridge:<session-id>"` |
| `message` | string or StructuredMessage | yes | Content to send |
| `summary` | string | no | 5-10 word preview (required for plain text) |

### Input validation rules

- `to` must not be empty
- `to` must not contain `@` (reserved)
- `summary` is required when `message` is a plain string
- Structured messages cannot be broadcast (`to: "*"`)
- Structured messages cannot cross sessions (`uds:` / `bridge:`)
- `shutdown_response` must target the team lead
- Rejection (`approve: false`) requires a `reason` / `feedback`

### StructuredMessage types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StructuredMessage {
    ShutdownRequest { reason: Option<String> },
    ShutdownResponse { request_id: String, approve: bool, reason: Option<String> },
    PlanApprovalResponse { request_id: String, approve: bool, feedback: Option<String> },
}
```

Defined in `src/services/mailbox/types.rs`.

### Message routing (5 paths)

| `to` pattern | Route | Mechanism |
|--------------|-------|-----------|
| agent name | In-process | See "In-process routing" below |
| `*` | Broadcast | Enumerate team file → write to each inbox |
| teammate name | File mailbox | `~/.atom/teams/{team}/inboxes/{name}.json` |
| `uds:<path>` | Unix socket | Plain text only. `tokio::net::UnixStream` |
| `bridge:<id>` | Remote session | Plain text only. HTTP POST |

### In-process agent routing (detailed)

1. Look up `to` in `app_state.agent_name_registry` → get `agent_id`
2. If not found, try `to` directly as raw agent_id
3. Find matching `BackgroundTask` in `app_state.background_tasks`:
   - **Running** → push message to `app_state.agent_message_queues[agent_id]`
   - **Stopped/Completed** → attempt resume: create new BackgroundTask with message as prompt, using transcript from `~/.atom/sessions/{session_id}/agents/{agent_id}/`
   - **Not found in memory** → attempt resume from disk transcript (same as stopped)
4. If no task found at all → fall back to file mailbox

### Team name resolution

Team name comes from `app_state.team_name: Option<String>` (new field).
Set by CLI `--team` flag or `ATOM_CODE_TEAM` env var.
When `None`, broadcast and teammate-name routing return an error.

### Structured message handlers

- **shutdown_request** → send request, return `request_id` (uuid)
- **shutdown_response(approve=true)** → in-process: `cancel_token.cancel()`; external: graceful shutdown
- **shutdown_response(approve=false)** → forward rejection reason
- **plan_approval_response(approve=true)** → inherit permission mode, approve execution
- **plan_approval_response(approve=false)** → forward feedback

### Output variants

```rust
MessageOutput { success, message, routing: { sender, target, summary, content } }
BroadcastOutput { success, message, recipients: Vec<String> }
RequestOutput { success, message, request_id, target }
ResponseOutput { success, message, request_id }
```

### Activation gate

```rust
fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
    // Two-level check:
    // 1. Free function (no state): is_agent_swarms_enabled() — env vars only
    // 2. AppState check: app_state.swarm_agents.is_empty() == false
    // Either being true enables the tool.
}
```

`is_agent_swarms_enabled()` defined as a free function in `src/services/coordinator/mod.rs`
(env-var only, no AppState access):

```rust
pub fn is_agent_swarms_enabled() -> bool {
    is_coordinator_mode()
        || matches!(
            std::env::var("ATOM_CODE_AGENT_SWARMS").unwrap_or_default().as_str(),
            "1" | "true" | "yes"
        )
}
```

The `can_use()` impl additionally checks `app_state.swarm_agents` at runtime since
the free function has no AppState access.

### Properties

- `name() = "SendMessage"`
- `is_read_only() = false`
- `should_defer() = true`
- `search_hint() = Some("send messages to agent teammates (swarm protocol)")`

**Note:** TypeScript makes `isReadOnly` dynamic (true for plain text, false for structured).
The Rust Tool trait takes `&self` without input access. Noted as a known limitation —
SendMessage is conservatively `is_read_only() = false`.

---

## 4. Mailbox Service (new)

**Location:** `src/services/mailbox/mod.rs`, `src/services/mailbox/types.rs`

```rust
pub struct Mailbox { base_dir: PathBuf }

impl Mailbox {
    pub fn new(team_name: &str) -> Self;           // base: ~/.atom/teams/{team}/inboxes/
    pub fn write(&self, recipient: &str, msg: MailboxMessage) -> Result<()>;
    pub fn read(&self, agent_name: &str) -> Result<Vec<MailboxMessage>>;
    pub fn clear(&self, agent_name: &str) -> Result<()>;
}

pub struct MailboxMessage {
    pub from: String,
    pub text: String,
    pub summary: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

File-based storage with `fs2` locking (same pattern as `TaskListStore`).

**Async note:** Mailbox methods are sync (fs2 locks are blocking). Callers must wrap
in `tokio::task::spawn_blocking` to avoid blocking the async runtime, same as TaskListStore.

---

## 5. SyntheticOutputTool (StructuredOutput)

**File:** `src/tools/synthetic_output_tool.rs`

### Stateful struct

Unlike other tools, SyntheticOutputTool holds the caller-provided JSON schema so that
`input_schema()` returns it to the LLM (the LLM needs to see what fields to produce):

```rust
pub struct SyntheticOutputTool {
    schema: serde_json::Value,
    compiled: Arc<jsonschema::JSONSchema>,
}

impl SyntheticOutputTool {
    pub fn new(json_schema: serde_json::Value) -> Result<Self> {
        let compiled = jsonschema::JSONSchema::compile(&json_schema)
            .map_err(|e| ...)?;
        Ok(Self { schema: json_schema, compiled: Arc::new(compiled) })
    }
}
```

### Dynamic input_schema

```rust
fn input_schema(&self) -> serde_json::Value {
    self.schema.clone()  // Returns the caller's schema, not a generic passthrough
}
```

This way the LLM sees the exact fields it needs to produce.

### Execution flow

1. Validate `ctx.input` against `self.compiled`
2. Pass → return `{ "data": "Structured output provided successfully", "structured_output": <input> }`
3. Fail → `ToolResult { is_error: true }` with validation error details

### Registration

SyntheticOutputTool is NOT registered in `get_builtin_tools()` (it requires runtime config).
Instead, it's conditionally added to the tools list when `--output-schema` is provided:

```rust
// In session setup (query engine or CLI init)
if let Some(schema) = cli_args.output_schema {
    let tool = SyntheticOutputTool::new(schema)?;
    tools.push(Box::new(tool));
}
```

### AppState addition

```rust
pub output_json_schema: Option<serde_json::Value>,  // raw schema for reference
```

No compiled schema in AppState — compilation happens in `SyntheticOutputTool::new()`.

### Properties

- `name() = "StructuredOutput"`
- `is_read_only() = true`
- `is_concurrent_safe() = true`
- `search_hint() = Some("return the final response as structured JSON")`

### Dependency

`jsonschema` crate added to `Cargo.toml`.

---

## 6. BriefTool (SendUserMessage)

**File:** `src/tools/brief_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `message` | string | yes | Markdown-supported user message |
| `attachments` | string[] | no | File paths (absolute or cwd-relative) |
| `status` | string | yes | `"normal"` or `"proactive"` |

### Execution flow

1. Extract `message`, parse `status`
2. Resolve attachments:
   - Relative → absolute via `ctx.cwd`
   - `tokio::fs::metadata` for existence, size, file type
   - Image detection by extension (png/jpg/jpeg/gif/webp/svg)
   - Missing file → include error in attachment entry, don't fail entire tool
3. Emit `QueryEvent::BriefMessage` via `ctx.event_tx`
4. Return output JSON

### Output

```json
{
  "message": "...",
  "attachments": [{ "path": "/abs/path.png", "size": 1234, "isImage": true }],
  "sentAt": "2026-04-04T12:00:00Z"
}
```

**Note:** TypeScript includes optional `file_uuid` for bridge-mode uploads.
Omitted for now — bridge upload support deferred to when bridge mode is fully implemented.

### QueryEvent addition (`src/query/types.rs`)

```rust
QueryEvent::BriefMessage {
    message: String,
    attachments: Vec<BriefAttachment>,
    status: BriefStatus,
}

#[derive(Debug, Clone)]
pub struct BriefAttachment { pub path: PathBuf, pub size: u64, pub is_image: bool }

#[derive(Debug, Clone)]
pub enum BriefStatus { Normal, Proactive }
```

### Activation

```rust
fn can_use(&self, ctx: &ToolUseContext) -> ValidationResult {
    // app_state.is_brief_only (--brief flag, defaultView: 'chat')
    // || app_state.kairos_enabled
    // Brief entitlement check simplified: no GrowthBook, just the above flags
}
```

### Properties

- `name() = "SendUserMessage"`, `aliases() = &["Brief"]`
- `is_read_only() = true`
- `is_concurrent_safe() = true`
- `always_load() = true` (must be available on turn 1, no ToolSearch round-trip)
- `search_hint() = Some("send a visible message to the user")`

---

## 7. AppState Changes Summary

```rust
// New fields in AppState
pub agent_message_queues: HashMap<String, Vec<serde_json::Value>>,  // in-process msg queues
pub team_name: Option<String>,                                       // team context
pub output_json_schema: Option<serde_json::Value>,                   // structured output schema
```

---

## 8. Implementation Order

1. Tool trait extension + `is_deferred_tool()` + `ToolDefinition.defer_loading` + `ToolResultBlock` extension
2. ToolSearchTool (select, exact-name, mcp-prefix, keyword, scoring)
3. `is_agent_swarms_enabled()` + Mailbox service → SendMessageTool (5 routing paths + structured messages + validation)
4. SyntheticOutputTool (stateful struct, jsonschema validation, conditional registration)
5. BriefTool (message + attachments + QueryEvent)

---

## 9. New Dependencies

| Crate | Purpose |
|-------|---------|
| `jsonschema` | JSON Schema validation for SyntheticOutputTool |

`fs2` and `tokio::net::UnixStream` already available or in std.

---

## 10. Known Limitations

- **SendMessage `is_read_only` is static:** TypeScript makes it input-dependent (true for plain text). Rust trait takes `&self` without input. Conservatively set to `false`.
- **BriefTool entitlement simplified:** No GrowthBook feature flag checks. Uses `is_brief_only` and `kairos_enabled` flags only.
- **Bridge file upload deferred:** BriefTool `file_uuid` attachment field omitted until bridge mode is fully implemented.
- **SyntheticOutputTool not in `get_builtin_tools()`:** It's conditionally added at session setup. The coordinator module's `COORDINATOR_ALLOWED_TOOLS` list includes `"SyntheticOutput"` but it may not exist in all sessions.
