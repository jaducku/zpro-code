# Batch 5: MCP Tools — Design Spec

**Date:** 2026-04-04
**Scope:** ListMcpResourcesTool, ReadMcpResourceTool, McpAuthTool
**Approach:** Tool-First — leveraging existing MCP infrastructure in `src/services/mcp/`

**Note:** MCPTool (base template) is already implemented as `McpToolWrapper` in `src/services/mcp/tool_wrapper.rs`.

---

## 1. ToolUseContext Extension

Add MCP connection manager access to `ToolUseContext` in `src/tool.rs`:

```rust
pub mcp_manager: Option<Arc<crate::services::mcp::connection_manager::McpConnectionManager>>,
```

Update `ToolUseContext::test()` to set `mcp_manager: None`.
Update query engine to pass the manager through.

---

## 2. ListMcpResourcesTool

**File:** `src/tools/list_mcp_resources_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `server` | string | no | Filter by server name |

### Execution

1. Get `mcp_manager` from ctx (error if None)
2. If `server` specified: get that client only; else get all connected clients
3. For each client, call `list_resources()` via `spawn_blocking` (client uses Mutex)
4. Collect results as `[{ uri, name, mimeType, description, server }]`
5. Empty results → helpful message about no resources

### Output

JSON array of resource objects, or message if empty.

### Properties

- `name() = "ListMcpResources"`
- `is_read_only() = true`
- `is_concurrent_safe() = true`
- `should_defer() = true`
- `search_hint() = Some("list available resources from MCP servers")`

---

## 3. ReadMcpResourceTool

**File:** `src/tools/read_mcp_resource_tool.rs`

### Input

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `server` | string | yes | MCP server name |
| `uri` | string | yes | Resource URI to read |

### Execution

1. Get `mcp_manager`, extract `server` and `uri` (both required)
2. Get client for the specified server
3. Call `read_resource(uri)` via the client
4. Process response contents:
   - Text content → include as `text` field
   - Binary (base64 blob) → decode, save to `~/.atom/sessions/{session_id}/blobs/{filename}`, return path as `blobSavedTo`
5. Return output

### Binary blob persistence

```rust
fn persist_binary_content(session_id: &str, uri: &str, data: &[u8]) -> Result<PathBuf> {
    let blobs_dir = crate::utils::paths::sessions_dir()
        .join(session_id)
        .join("blobs");
    std::fs::create_dir_all(&blobs_dir)?;
    // Extract filename from URI or generate from hash
    let filename = uri.rsplit('/').next().unwrap_or("blob");
    let path = blobs_dir.join(filename);
    std::fs::write(&path, data)?;
    Ok(path)
}
```

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

## 4. OAuth Service

**File:** `src/services/mcp/oauth.rs`

```rust
pub struct McpOAuthService;

pub enum OAuthResult {
    AuthUrl(String),
    Unsupported,
}

impl McpOAuthService {
    /// Perform OAuth discovery and return authorization URL.
    /// Does NOT open browser — returns URL for user to visit.
    pub async fn perform_oauth_flow(
        server_name: &str,
        config: &McpServerConfig,
    ) -> Result<OAuthResult> {
        // 1. Check transport type supports OAuth (sse, http)
        // 2. Discover OAuth metadata from .well-known/oauth-authorization-server
        // 3. Build authorization URL with PKCE
        // 4. Return URL
    }
}
```

### OAuth discovery

- Fetch `{server_url}/.well-known/oauth-authorization-server`
- Extract `authorization_endpoint`, `token_endpoint`
- Generate PKCE code_verifier + code_challenge
- Build auth URL with `client_id`, `redirect_uri`, `code_challenge`, `scope`

### Known limitations

- OAuth callback server (localhost redirect) not implemented — URL returned only
- Token storage/refresh deferred to when keychain integration is active
- Only basic OAuth discovery (`.well-known`) supported
- stdio transport servers don't support OAuth (return `Unsupported`)

---

## 5. McpAuthTool

**File:** `src/tools/mcp_auth_tool.rs`

### Stateful struct (factory pattern)

```rust
pub struct McpAuthTool {
    server_name: String,
    server_config: McpServerConfig,
}

impl McpAuthTool {
    pub fn new(server_name: String, config: McpServerConfig) -> Self;
}
```

### Input

`{ "type": "object", "properties": {} }` — no parameters

### Execution

1. Call `McpOAuthService::perform_oauth_flow(&self.server_name, &self.server_config)`
2. Match result:
   - `AuthUrl(url)` → return `{ status: "auth_url", message: "...", authUrl: url }`
   - `Unsupported` → return `{ status: "unsupported", message: "..." }`
3. On error → return `{ status: "error", message: "..." }`

### Dynamic registration

NOT in `get_builtin_tools()`. Created when MCP server returns `needs-auth`:

```rust
// In McpConnectionManager, when server needs auth:
let auth_tool = McpAuthTool::new(server_name.clone(), config.clone());
// Name: mcp__{server}__authenticate
// Added to dynamic tools list
```

After auth succeeds, prefix-based swap removes auth tool, loads real tools.

### Properties

- `name()` = dynamic: `format!("mcp__{server}__authenticate")`
- `is_mcp() = true`
- `is_read_only() = true`
- `should_defer() = false` (must be visible immediately when auth is needed)

---

## 6. Implementation Order

1. `ToolUseContext.mcp_manager` + engine plumbing
2. ListMcpResourcesTool
3. ReadMcpResourceTool (with binary blob persistence)
4. OAuth service (`src/services/mcp/oauth.rs`)
5. McpAuthTool

---

## 7. New Dependencies

None — `base64` and `reqwest` already in Cargo.toml.

---

## 8. Known Limitations

- **OAuth callback server not implemented** — returns URL only, user authenticates manually
- **Token persistence deferred** — no keychain storage for OAuth tokens yet
- **OAuth discovery basic** — only `.well-known/oauth-authorization-server`, no fallback
- **stdio servers don't support OAuth** — McpAuthTool returns `unsupported` for them
- **Binary blob path collision** — if two resources have same filename, later overwrites earlier (acceptable for v1)
- **McpConnectionManager locking** — client access uses `Arc<Mutex<McpClient>>`, `spawn_blocking` for resource calls
