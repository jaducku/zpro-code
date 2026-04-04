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
    pub defer_loading: bool,  // NEW — signals API to omit full schema
}
```

`From<&dyn Tool>` sets `defer_loading` based on `is_deferred_tool()`.

---

## 2. ToolSearchTool

**File:** `src/tools/tool_search_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `query` | string | yes | `"select:Read,Edit"`, `"+slack send"`, or keyword search |
| `max_results` | number | no | Default 5 |

### Query modes

1. **`select:Name1,Name2`** — exact name match from `ctx.tools`
2. **`+required optional`** — `+` prefix terms must all match, rest ranked by score
3. **keyword search** — all terms optional, ranked by score

### Scoring (deferred tools only)

| Match location | Full word | Partial |
|----------------|-----------|---------|
| name (MCP) | +12 | +6 |
| name (regular) | +10 | +5 |
| search_hint | +4 | — |
| description | +2 | — |

### Output format

```xml
<functions>
<function>{"description": "...", "name": "...", "parameters": {...}}</function>
</functions>
```

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
| agent name | In-process | `agent_name_registry` → BackgroundTask → message queue. Auto-resume if stopped. |
| `*` | Broadcast | Enumerate team file → write to each inbox |
| teammate name | File mailbox | `~/.atom/teams/{team}/inboxes/{name}.json` |
| `uds:<path>` | Unix socket | Plain text only. `tokio::net::UnixStream` |
| `bridge:<id>` | Remote session | Plain text only. HTTP POST |

### Structured message handlers

- **shutdown_request** → send request, return `request_id`
- **shutdown_response(approve=true)** → in-process: `cancel_token` signal; external: graceful shutdown
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

### Properties

- `name() = "SendMessage"`
- `is_read_only() = false`
- `should_defer() = true`
- `can_use()` → gated on `is_agent_swarms_enabled()`

---

## 4. Mailbox Service (new)

**Location:** `src/services/mailbox/mod.rs`, `src/services/mailbox/types.rs`

```rust
pub struct Mailbox { base_dir: PathBuf }

impl Mailbox {
    pub fn new(team_name: &str) -> Self;           // base: ~/.atom/teams/{team}/inboxes/
    pub fn write(&self, recipient: &str, msg: MailboxMessage) -> Result<()>;  // fs2 file lock
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

---

## 5. SyntheticOutputTool (StructuredOutput)

**File:** `src/tools/synthetic_output_tool.rs`

### Input

Passthrough object — `{"type": "object", "additionalProperties": true}`.

### Execution flow

1. Read `output_json_schema` from `ctx.app_state`
2. Validate input against schema using `jsonschema` crate
3. Pass → return `{ "data": "Structured output provided successfully", "structured_output": <input> }`
4. Fail → `ToolResult { is_error: true }` with validation errors

### AppState additions

```rust
pub output_json_schema: Option<serde_json::Value>,
#[serde(skip)]
pub compiled_output_schema: Option<Arc<jsonschema::JSONSchema>>,
```

### Activation

`can_use()` checks `app_state.output_json_schema.is_some()` (non-interactive session with `--output-schema`).

### Properties

- `name() = "StructuredOutput"`
- `is_read_only() = true`
- `is_concurrent_safe() = true`

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

### QueryEvent addition (`src/query/types.rs`)

```rust
QueryEvent::BriefMessage {
    message: String,
    attachments: Vec<BriefAttachment>,
    status: BriefStatus,
}

pub struct BriefAttachment { pub path: PathBuf, pub size: u64, pub is_image: bool }
pub enum BriefStatus { Normal, Proactive }
```

### Activation

`can_use()` checks `app_state.is_brief_only || app_state.kairos_enabled`.

### Properties

- `name() = "SendUserMessage"`, `aliases() = &["Brief"]`
- `is_read_only() = true`
- `is_concurrent_safe() = true`
- `should_defer() = true`

---

## 7. AppState Changes Summary

```rust
// New fields in AppState
pub agent_message_queues: HashMap<String, Vec<serde_json::Value>>,  // in-process msg queues
pub output_json_schema: Option<serde_json::Value>,                   // structured output schema
#[serde(skip)]
pub compiled_output_schema: Option<Arc<jsonschema::JSONSchema>>,     // cached validator
```

---

## 8. Implementation Order

1. Tool trait extension + `is_deferred_tool()` + `ToolDefinition.defer_loading`
2. ToolSearchTool (select, keyword, scoring)
3. Mailbox service → SendMessageTool (5 routing paths + structured messages)
4. SyntheticOutputTool (jsonschema validation)
5. BriefTool (message + attachments + QueryEvent)

---

## 9. New Dependencies

| Crate | Purpose |
|-------|---------|
| `jsonschema` | JSON Schema validation for SyntheticOutputTool |

`fs2` and `tokio::net::UnixStream` already available or in std.
