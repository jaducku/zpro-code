# Batch 5: MCP Resource Tools — Design Spec

**Date:** 2026-04-04
**Scope:** ListMcpResourcesTool, ReadMcpResourceTool (2 tools + McpConnectionManager accessor methods)
**Approach:** Tool-First — leveraging existing MCP infrastructure in `src/services/mcp/`

**Note:** MCPTool (base template) is already implemented as `McpToolWrapper` in `src/services/mcp/tool_wrapper.rs`.

**Deferred:** McpAuthTool + OAuth service deferred to a future batch that first adds HTTP/SSE transport support to `McpServerConfig` (currently stdio-only).

---

## 1. ToolUseContext Extension

Add MCP connection manager access to `ToolUseContext` in `src/tool.rs`:

```rust
pub mcp_manager: Option<Arc<crate::services::mcp::connection_manager::McpConnectionManager>>,
```

Update `ToolUseContext::test()` to set `mcp_manager: None`.
Update query engine to pass the manager through.

---

## 2. McpConnectionManager Accessor Methods

The existing `McpConnectionManager` has a private `clients` field. Add public accessors
(in `src/services/mcp/connection_manager.rs`):

```rust
/// Get a specific client by server name
pub fn get_client(&self, name: &str) -> Option<Arc<tokio::sync::Mutex<McpClient>>> {
    self.clients.get(name).cloned()
}

/// Get all connected clients with their names
pub fn get_all_clients(&self) -> Vec<(String, Arc<tokio::sync::Mutex<McpClient>>)> {
    self.clients.iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
```

**Important:** `McpConnectionManager` uses `tokio::sync::Mutex` (not std::sync::Mutex).
All client access must use `.lock().await` — NOT `spawn_blocking`.

---

## 3. ListMcpResourcesTool

**File:** `src/tools/list_mcp_resources_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `server` | string | no | Filter by server name |

### Execution

1. Get `mcp_manager` from ctx (error if None — "No MCP servers configured")
2. If `server` specified: `get_client(server)`, error if not found
3. Else: `get_all_clients()`
4. For each client:
   a. Check `server.capabilities.resources` is Some (skip servers that don't support resources)
   b. Call `client.lock().await.list_resources()` (async, no spawn_blocking)
5. Collect results as array, renaming `mime_type` → `mimeType` in JSON output:
   ```json
   [{ "uri": "...", "name": "...", "mimeType": "...", "description": "...", "server": "..." }]
   ```
6. Empty results → `"No resources found. MCP servers may still provide tools even if they have no resources."`

### Properties

- `name() = "ListMcpResources"`
- `is_read_only() = true`
- `is_concurrent_safe() = true`
- `should_defer() = true`
- `search_hint() = Some("list resources from connected MCP servers")`

---

## 4. ReadMcpResourceTool

**File:** `src/tools/read_mcp_resource_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `server` | string | yes | MCP server name |
| `uri` | string | yes | Resource URI to read |

### Execution

1. Get `mcp_manager`, extract `server` and `uri` (both required)
2. Get client for the specified server (`get_client`), error if not found
3. Check `server.capabilities.resources` is Some, else error
4. Call `client.lock().await.read_resource(uri)` (async, no spawn_blocking)
5. Process response contents:
   - Text content → include as `text` field
   - Binary (base64 blob) → decode with `base64` crate, persist to disk, return path as `blobSavedTo`
   - Persistence failure → include error message in `text` field (don't fail entire tool)

### Binary blob persistence

Use unique persist ID (timestamp + index) to avoid collisions. Derive extension from `mime_type`:

```rust
fn persist_binary_content(
    session_id: &str,
    mime_type: Option<&str>,
    data: &[u8],
    index: usize,
) -> Result<PathBuf> {
    let blobs_dir = crate::utils::paths::sessions_dir()
        .join(session_id)
        .join("blobs");
    std::fs::create_dir_all(&blobs_dir)?;

    let ext = mime_type_to_extension(mime_type);
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
    let filename = format!("blob-{}-{}.{}", timestamp, index, ext);
    let path = blobs_dir.join(filename);
    std::fs::write(&path, data)?;
    Ok(path)
}

fn mime_type_to_extension(mime_type: Option<&str>) -> &str {
    match mime_type {
        Some("image/png") => "png",
        Some("image/jpeg") => "jpg",
        Some("image/gif") => "gif",
        Some("image/webp") => "webp",
        Some("application/pdf") => "pdf",
        Some("text/plain") => "txt",
        Some("text/html") => "html",
        Some("application/json") => "json",
        _ => "bin",
    }
}
```

`session_id` accessed via `ctx.app_state.read().await.session_id.clone()`.

### Output

```json
{
  "contents": [
    { "uri": "file:///path", "mimeType": "text/plain", "text": "content..." },
    { "uri": "file:///img.png", "mimeType": "image/png", "blobSavedTo": "/path/to/blob" }
  ]
}
```

### Properties

- `name() = "ReadMcpResource"`
- `is_read_only() = true`
- `is_concurrent_safe() = true`
- `should_defer() = true`
- `search_hint() = Some("read a specific resource from an MCP server by URI")`

---

## 5. Implementation Order

1. McpConnectionManager accessor methods (`get_client`, `get_all_clients`)
2. `ToolUseContext.mcp_manager` + engine plumbing
3. ListMcpResourcesTool
4. ReadMcpResourceTool (with binary blob persistence)

---

## 6. New Dependencies

None — `base64`, `chrono`, `reqwest` already in Cargo.toml.

---

## 7. Known Limitations

- **McpAuthTool deferred** — requires HTTP/SSE transport support in `McpServerConfig` (currently stdio-only). Will be implemented in a future batch that adds multi-transport config.
- **Client access is async** — uses `tokio::sync::Mutex`, all access via `.lock().await`. No `spawn_blocking`.
- **Capabilities check** — tools check `server.capabilities.resources` before making resource calls. Servers without resource support are skipped (list) or rejected (read).
- **Binary blob unique naming** — uses timestamp + index pattern to avoid collisions.
- **`mime_type` → `mimeType` rename** — `McpResource.mime_type` (snake_case in Rust) is renamed to `mimeType` (camelCase) in JSON output to match the MCP protocol.
