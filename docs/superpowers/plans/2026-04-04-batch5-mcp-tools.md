# Batch 5: MCP Resource Tools — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add 2 MCP resource tools (ListMcpResources, ReadMcpResource) with connection manager accessors and ToolUseContext plumbing.

**Architecture:** Tool-first. Leverages existing MCP infrastructure (`McpClient`, `McpConnectionManager`). Adds accessor methods for client lookup, threads `mcp_manager` through `ToolUseContext`. Uses `tokio::sync::Mutex` (async) for all client access — no `spawn_blocking`.

**Tech Stack:** Rust, async-trait, tokio, tokio::sync::Mutex, base64, chrono, serde

**Spec:** `docs/superpowers/specs/2026-04-04-batch5-mcp-tools-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/services/mcp/connection_manager.rs` | Modify | Add `get_client()`, `get_all_clients()` |
| `src/tool.rs` | Modify | Add `mcp_manager` to `ToolUseContext` |
| `src/query/engine.rs` | Modify | Pass `mcp_manager` through to ToolUseContext |
| `src/tools/list_mcp_resources_tool.rs` | Create | ListMcpResourcesTool |
| `src/tools/read_mcp_resource_tool.rs` | Create | ReadMcpResourceTool with blob persistence |
| `src/tools/mod.rs` | Modify | Register 2 new tools |

---

### Task 1: McpConnectionManager Accessor Methods

**Files:**
- Modify: `src/services/mcp/connection_manager.rs`

- [ ] **Step 1: Add `get_client` and `get_all_clients` methods**

In `src/services/mcp/connection_manager.rs`, add after `connected_servers()` (line 101):

```rust
    /// Get a specific client by server name
    pub fn get_client(&self, name: &str) -> Option<Arc<Mutex<McpClient>>> {
        self.clients.get(name).cloned()
    }

    /// Get all connected clients with their server names
    pub fn get_all_clients(&self) -> Vec<(String, Arc<Mutex<McpClient>>)> {
        self.clients
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
```

- [ ] **Step 2: Add tests**

```rust
#[test]
fn test_get_client_not_found() {
    let manager = McpConnectionManager::new(HashMap::new());
    assert!(manager.get_client("nonexistent").is_none());
}

#[test]
fn test_get_all_clients_empty() {
    let manager = McpConnectionManager::new(HashMap::new());
    assert!(manager.get_all_clients().is_empty());
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code connection_manager 2>&1 | tail -10`
Expected: All pass.

- [ ] **Step 4: Commit**

```bash
git add src/services/mcp/connection_manager.rs
git commit -m "feat: add get_client and get_all_clients to McpConnectionManager"
```

---

### Task 2: ToolUseContext.mcp_manager + Engine Plumbing

**Files:**
- Modify: `src/tool.rs`
- Modify: `src/query/engine.rs`

- [ ] **Step 1: Add `mcp_manager` to ToolUseContext**

In `src/tool.rs`, add field to `ToolUseContext` after `app_state` (around line 79):

```rust
    pub mcp_manager: Option<Arc<crate::services::mcp::connection_manager::McpConnectionManager>>,
```

Update `ToolUseContext::test()` to set `mcp_manager: None`.

- [ ] **Step 2: Update query engine**

In `src/query/engine.rs`, where `ToolUseContext` is constructed for tool execution, add `mcp_manager: None` (or pass through from QueryContext if available). Find the construction site by searching for `ToolUseContext {` in `engine.rs`.

If `QueryContext` doesn't have `mcp_manager`, add it there too:

```rust
// In src/query/types.rs, add to QueryContext:
pub mcp_manager: Option<Arc<crate::services::mcp::connection_manager::McpConnectionManager>>,
```

Then thread it through in engine.rs where ToolUseContext is built.

- [ ] **Step 3: Fix compilation — update all ToolUseContext construction sites**

The engine and any test files that construct ToolUseContext directly will need the new field. Search for all `ToolUseContext {` constructions and add `mcp_manager: None` (or the threaded value).

- [ ] **Step 4: Run tests**

Run: `cargo test -p atom-code 2>&1 | tail -10`
Expected: All existing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src/tool.rs src/query/engine.rs src/query/types.rs
git commit -m "feat: add mcp_manager to ToolUseContext and query engine plumbing"
```

---

### Task 3: ListMcpResourcesTool

**Files:**
- Create: `src/tools/list_mcp_resources_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Implement ListMcpResourcesTool**

Create `src/tools/list_mcp_resources_tool.rs`:

```rust
use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::AtomError;

pub struct ListMcpResourcesTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "ListMcpResources".into(),
        message: msg.into(),
    }
}

#[async_trait]
impl Tool for ListMcpResourcesTool {
    fn name(&self) -> &str {
        "ListMcpResources"
    }

    fn description(&self) -> &str {
        "List available resources from connected MCP servers."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "Filter by server name (optional)"
                }
            }
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let manager = ctx.mcp_manager.as_ref()
            .ok_or_else(|| tool_err("No MCP servers configured"))?;

        let server_filter = ctx.input.get("server").and_then(|v| v.as_str());

        let clients = if let Some(name) = server_filter {
            let client = manager.get_client(name)
                .ok_or_else(|| tool_err(format!("MCP server '{}' not found", name)))?;
            vec![(name.to_string(), client)]
        } else {
            manager.get_all_clients()
        };

        let mut resources = Vec::new();

        for (server_name, client_arc) in &clients {
            let mut client = client_arc.lock().await;

            // Check capabilities — skip servers that don't support resources
            if client.capabilities.resources.is_none() {
                continue;
            }

            match client.list_resources().await {
                Ok(server_resources) => {
                    for r in server_resources {
                        resources.push(serde_json::json!({
                            "uri": r.uri,
                            "name": r.name,
                            "mimeType": r.mime_type,
                            "description": r.description,
                            "server": server_name,
                        }));
                    }
                }
                Err(e) => {
                    tracing::warn!(server = %server_name, error = %e, "Failed to list resources");
                }
            }
        }

        let content = if resources.is_empty() {
            "No resources found. MCP servers may still provide tools even if they have no resources.".to_string()
        } else {
            serde_json::to_string_pretty(&resources).unwrap_or_default()
        };

        Ok(ToolResult {
            content,
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> {
        Some("list resources from connected MCP servers")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(ListMcpResourcesTool.name(), "ListMcpResources");
    }

    #[test]
    fn test_schema() {
        let schema = ListMcpResourcesTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["server"].is_object());
    }

    #[test]
    fn test_properties() {
        assert!(ListMcpResourcesTool.is_read_only());
        assert!(ListMcpResourcesTool.is_concurrent_safe());
        assert!(ListMcpResourcesTool.should_defer());
    }

    #[tokio::test]
    async fn test_no_mcp_manager() {
        let tool = ListMcpResourcesTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Register in mod.rs**

Add `pub mod list_mcp_resources_tool;` and `Box::new(list_mcp_resources_tool::ListMcpResourcesTool)` to `get_builtin_tools()`.

Update `test_deferred_tools` expected list to include `"ListMcpResources"`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code list_mcp 2>&1 | tail -15`
Expected: All pass.

- [ ] **Step 4: Commit**

```bash
git add src/tools/list_mcp_resources_tool.rs src/tools/mod.rs
git commit -m "feat: implement ListMcpResourcesTool"
```

---

### Task 4: ReadMcpResourceTool

**Files:**
- Create: `src/tools/read_mcp_resource_tool.rs`
- Modify: `src/tools/mod.rs`

- [ ] **Step 1: Implement ReadMcpResourceTool with blob persistence**

Create `src/tools/read_mcp_resource_tool.rs`:

```rust
use async_trait::async_trait;
use std::path::PathBuf;

use crate::tool::{Tool, ToolResult, ToolUseContext, ValidationResult};
use crate::types::AtomError;

pub struct ReadMcpResourceTool;

fn tool_err(msg: impl Into<String>) -> AtomError {
    AtomError::Tool {
        tool_name: "ReadMcpResource".into(),
        message: msg.into(),
    }
}

fn mime_type_to_extension(mime_type: Option<&str>) -> &str {
    match mime_type {
        Some("image/png") => "png",
        Some("image/jpeg") => "jpg",
        Some("image/gif") => "gif",
        Some("image/webp") => "webp",
        Some("image/svg+xml") => "svg",
        Some("application/pdf") => "pdf",
        Some("text/plain") => "txt",
        Some("text/html") => "html",
        Some("text/css") => "css",
        Some("text/javascript") | Some("application/javascript") => "js",
        Some("application/json") => "json",
        Some("application/xml") | Some("text/xml") => "xml",
        _ => "bin",
    }
}

fn persist_binary_content(
    session_id: &str,
    mime_type: Option<&str>,
    data: &[u8],
    index: usize,
) -> anyhow::Result<PathBuf> {
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

#[async_trait]
impl Tool for ReadMcpResourceTool {
    fn name(&self) -> &str {
        "ReadMcpResource"
    }

    fn description(&self) -> &str {
        "Read a specific resource from an MCP server by URI."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "server": {
                    "type": "string",
                    "description": "MCP server name"
                },
                "uri": {
                    "type": "string",
                    "description": "Resource URI to read"
                }
            },
            "required": ["server", "uri"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let manager = ctx.mcp_manager.as_ref()
            .ok_or_else(|| tool_err("No MCP servers configured"))?;

        let server = ctx.input.get("server")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'server' parameter"))?;

        let uri = ctx.input.get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required 'uri' parameter"))?;

        let client_arc = manager.get_client(server)
            .ok_or_else(|| tool_err(format!("MCP server '{}' not found", server)))?;

        // Check capabilities
        {
            let client = client_arc.lock().await;
            if client.capabilities.resources.is_none() {
                return Err(tool_err(format!(
                    "Server '{}' does not support resources", server
                )));
            }
        }

        // Read resource
        let contents = {
            let mut client = client_arc.lock().await;
            client.read_resource(uri).await
                .map_err(|e| tool_err(format!("Failed to read resource: {}", e)))?
        };

        let session_id = ctx.app_state.read().await.session_id.clone();

        let mut output_contents = Vec::new();
        for (i, content) in contents.iter().enumerate() {
            let mut entry = serde_json::json!({
                "uri": content.uri,
                "mimeType": content.mime_type,
            });

            if let Some(ref text) = content.text {
                entry["text"] = serde_json::Value::String(text.clone());
            }

            if let Some(ref blob_b64) = content.blob {
                // Decode base64 and persist
                match base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    blob_b64,
                ) {
                    Ok(data) => {
                        match persist_binary_content(
                            &session_id,
                            content.mime_type.as_deref(),
                            &data,
                            i,
                        ) {
                            Ok(path) => {
                                entry["blobSavedTo"] =
                                    serde_json::Value::String(path.display().to_string());
                            }
                            Err(e) => {
                                entry["text"] = serde_json::Value::String(format!(
                                    "Binary content could not be saved to disk: {}",
                                    e
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        entry["text"] = serde_json::Value::String(format!(
                            "Failed to decode base64 blob: {}",
                            e
                        ));
                    }
                }
            }

            output_contents.push(entry);
        }

        let response = serde_json::json!({ "contents": output_contents });

        Ok(ToolResult {
            content: serde_json::to_string_pretty(&response).unwrap_or_default(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }
    fn should_defer(&self) -> bool { true }
    fn search_hint(&self) -> Option<&str> {
        Some("read a specific resource from an MCP server by URI")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(ReadMcpResourceTool.name(), "ReadMcpResource");
    }

    #[test]
    fn test_schema() {
        let schema = ReadMcpResourceTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["server"].is_object());
        assert!(schema["properties"]["uri"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("server")));
        assert!(required.contains(&serde_json::json!("uri")));
    }

    #[test]
    fn test_properties() {
        assert!(ReadMcpResourceTool.is_read_only());
        assert!(ReadMcpResourceTool.is_concurrent_safe());
        assert!(ReadMcpResourceTool.should_defer());
    }

    #[test]
    fn test_mime_type_to_extension() {
        assert_eq!(mime_type_to_extension(Some("image/png")), "png");
        assert_eq!(mime_type_to_extension(Some("application/pdf")), "pdf");
        assert_eq!(mime_type_to_extension(Some("text/plain")), "txt");
        assert_eq!(mime_type_to_extension(None), "bin");
        assert_eq!(mime_type_to_extension(Some("unknown/type")), "bin");
    }

    #[tokio::test]
    async fn test_no_mcp_manager() {
        let tool = ReadMcpResourceTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "server": "test",
            "uri": "file:///test"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_server() {
        let tool = ReadMcpResourceTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "uri": "file:///test"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_uri() {
        let tool = ReadMcpResourceTool;
        let ctx = ToolUseContext::test(serde_json::json!({
            "server": "test"
        }));
        let result = tool.execute(ctx).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Register in mod.rs**

Add `pub mod read_mcp_resource_tool;` and `Box::new(read_mcp_resource_tool::ReadMcpResourceTool)` to `get_builtin_tools()`.

Update `test_deferred_tools` expected list to include `"ReadMcpResource"`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p atom-code read_mcp 2>&1 | tail -15`
Expected: All pass.

- [ ] **Step 4: Run full test suite**

Run: `cargo test -p atom-code 2>&1 | tail -10`
Expected: All existing tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src/tools/read_mcp_resource_tool.rs src/tools/mod.rs
git commit -m "feat: implement ReadMcpResourceTool with binary blob persistence"
```

---

### Task 5: Update Memory

- [ ] **Step 1: Update tool migration status**

Update memory file to reflect Batch 5 completion (30/54 tools).
