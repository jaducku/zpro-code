# Phase 5: MCP Client

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the MCP (Model Context Protocol) client with JSON-RPC 2.0 messaging, stdio transport, and connection management. MCP tools are wrapped as Tool trait objects and merged into the query engine.

**Architecture:** `McpTransport` trait for transport abstraction, `McpClient` for protocol logic, `McpConnectionManager` for lifecycle, `McpToolWrapper` to bridge MCP tools into the Tool trait.

**Tech Stack:** tokio (process, channels), serde_json, uuid

**Spec:** `docs/superpowers/specs/2026-04-02-ts-to-rust-migration-design.md` (Section 6)

---

### Task 1: JSON-RPC types

**Files:**
- Create: `src/services/mcp/jsonrpc.rs`
- Create: `src/services/mcp/mod.rs`
- Modify: `src/services/mod.rs`

- [ ] **Step 1: Define JSON-RPC 2.0 types**

```rust
// src/services/mcp/jsonrpc.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: JsonRpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcId {
    Number(i64),
    String(String),
}

/// Incoming message from MCP server (can be response or notification)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcMessage {
    Response(JsonRpcResponse),
    Notification(JsonRpcNotification),
}

impl JsonRpcRequest {
    pub fn new(id: i64, method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: JsonRpcId::Number(id),
            method: method.into(),
            params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = JsonRpcRequest::new(1, "initialize", Some(serde_json::json!({"capabilities": {}})));
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "initialize");
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_error_response() {
        let json = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid request"}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32600);
    }

    #[test]
    fn test_notification_deserialization() {
        let json = r#"{"jsonrpc":"2.0","method":"notifications/progress","params":{"progress":50}}"#;
        let notif: JsonRpcNotification = serde_json::from_str(json).unwrap();
        assert_eq!(notif.method, "notifications/progress");
    }
}
```

```rust
// src/services/mcp/mod.rs
pub mod jsonrpc;
pub mod transport;
pub mod client;
pub mod types;
pub mod connection_manager;
pub mod tool_wrapper;
```

- [ ] **Step 2: Add `pub mod mcp;` to services/mod.rs** (if not already present)

- [ ] **Step 3: Run tests, commit**

---

### Task 2: MCP types and transport trait

**Files:**
- Create: `src/services/mcp/types.rs`
- Create: `src/services/mcp/transport.rs`

- [ ] **Step 1: Define MCP protocol types**

```rust
// src/services/mcp/types.rs
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceContent {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    pub content: Vec<McpToolResultContent>,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpToolResultContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
}

/// Configuration for an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definition_deser() {
        let json = r#"{"name":"read_file","description":"Read a file","inputSchema":{"type":"object","properties":{"path":{"type":"string"}}}}"#;
        let tool: McpToolDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "read_file");
    }

    #[test]
    fn test_server_config() {
        let config = McpServerConfig {
            command: "npx".into(),
            args: vec!["-y".into(), "mcp-server".into()],
            env: std::collections::HashMap::new(),
        };
        let json = serde_json::to_value(&config).unwrap();
        assert_eq!(json["command"], "npx");
    }
}
```

- [ ] **Step 2: Define transport trait**

```rust
// src/services/mcp/transport.rs
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use super::jsonrpc::{JsonRpcMessage, JsonRpcRequest};
use super::types::McpServerConfig;
use crate::types::Result;

/// Transport abstraction for MCP communication
#[async_trait]
pub trait McpTransport: Send + Sync {
    async fn send(&mut self, request: &JsonRpcRequest) -> Result<()>;
    async fn receive(&mut self) -> Result<JsonRpcMessage>;
    async fn close(&mut self) -> Result<()>;
}

/// Stdio transport — communicates via child process stdin/stdout
pub struct StdioTransport {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
}

impl StdioTransport {
    pub async fn spawn(config: &McpServerConfig) -> Result<Self> {
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| crate::types::AtomError::Mcp {
            server_name: config.command.clone(),
            message: format!("Failed to spawn: {}", e),
        })?;

        let stdin = child.stdin.take().ok_or_else(|| crate::types::AtomError::Mcp {
            server_name: config.command.clone(),
            message: "Failed to capture stdin".into(),
        })?;

        let stdout = child.stdout.take().ok_or_else(|| crate::types::AtomError::Mcp {
            server_name: config.command.clone(),
            message: "Failed to capture stdout".into(),
        })?;

        Ok(Self {
            child,
            stdin,
            reader: BufReader::new(stdout),
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&mut self, request: &JsonRpcRequest) -> Result<()> {
        let json = serde_json::to_string(request)?;
        self.stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| crate::types::AtomError::Io(e))?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|e| crate::types::AtomError::Io(e))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| crate::types::AtomError::Io(e))?;
        Ok(())
    }

    async fn receive(&mut self) -> Result<JsonRpcMessage> {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .await
            .map_err(|e| crate::types::AtomError::Io(e))?;

        if line.is_empty() {
            return Err(crate::types::AtomError::Mcp {
                server_name: "stdio".into(),
                message: "Connection closed".into(),
            });
        }

        let msg: JsonRpcMessage = serde_json::from_str(line.trim())?;
        Ok(msg)
    }

    async fn close(&mut self) -> Result<()> {
        let _ = self.child.kill().await;
        Ok(())
    }
}
```

- [ ] **Step 3: Commit**

---

### Task 3: MCP client

**Files:**
- Create: `src/services/mcp/client.rs`

- [ ] **Step 1: Implement MCP client**

```rust
// src/services/mcp/client.rs
use std::sync::atomic::{AtomicI64, Ordering};

use super::jsonrpc::*;
use super::transport::McpTransport;
use super::types::*;
use crate::types::Result;

pub struct McpClient {
    transport: Box<dyn McpTransport>,
    next_id: AtomicI64,
    pub server_info: Option<ServerInfo>,
    pub capabilities: ServerCapabilities,
}

impl McpClient {
    pub fn new(transport: Box<dyn McpTransport>) -> Self {
        Self {
            transport,
            next_id: AtomicI64::new(1),
            server_info: None,
            capabilities: ServerCapabilities::default(),
        }
    }

    fn next_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a request and wait for response
    async fn request(&mut self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let id = self.next_id();
        let req = JsonRpcRequest::new(id, method, params);
        self.transport.send(&req).await?;

        // Read responses until we get the one matching our id
        loop {
            let msg = self.transport.receive().await?;
            match msg {
                JsonRpcMessage::Response(resp) => {
                    if let Some(error) = resp.error {
                        return Err(crate::types::AtomError::Mcp {
                            server_name: self.server_info.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
                            message: format!("JSON-RPC error {}: {}", error.code, error.message),
                        });
                    }
                    return Ok(resp.result.unwrap_or(serde_json::Value::Null));
                }
                JsonRpcMessage::Notification(_) => {
                    // Skip notifications, keep waiting for our response
                    continue;
                }
            }
        }
    }

    /// Initialize the MCP connection
    pub async fn initialize(&mut self) -> Result<()> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "atom-code",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let result = self.request("initialize", Some(params)).await?;

        if let Some(info) = result.get("serverInfo") {
            self.server_info = serde_json::from_value(info.clone()).ok();
        }
        if let Some(caps) = result.get("capabilities") {
            self.capabilities = serde_json::from_value(caps.clone()).unwrap_or_default();
        }

        // Send initialized notification
        let notif = JsonRpcRequest::new(self.next_id(), "notifications/initialized", None);
        self.transport.send(&notif).await?;

        Ok(())
    }

    /// List available tools
    pub async fn list_tools(&mut self) -> Result<Vec<McpToolDefinition>> {
        let result = self.request("tools/list", None).await?;
        let tools: Vec<McpToolDefinition> = result
            .get("tools")
            .and_then(|t| serde_json::from_value(t.clone()).ok())
            .unwrap_or_default();
        Ok(tools)
    }

    /// Call a tool
    pub async fn call_tool(&mut self, name: &str, arguments: serde_json::Value) -> Result<McpToolResult> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });
        let result = self.request("tools/call", Some(params)).await?;
        let tool_result: McpToolResult = serde_json::from_value(result).map_err(|e| {
            crate::types::AtomError::Mcp {
                server_name: self.server_info.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
                message: format!("Failed to parse tool result: {}", e),
            }
        })?;
        Ok(tool_result)
    }

    /// List available resources
    pub async fn list_resources(&mut self) -> Result<Vec<McpResource>> {
        let result = self.request("resources/list", None).await?;
        let resources: Vec<McpResource> = result
            .get("resources")
            .and_then(|r| serde_json::from_value(r.clone()).ok())
            .unwrap_or_default();
        Ok(resources)
    }

    /// Read a resource
    pub async fn read_resource(&mut self, uri: &str) -> Result<Vec<McpResourceContent>> {
        let params = serde_json::json!({"uri": uri});
        let result = self.request("resources/read", Some(params)).await?;
        let contents: Vec<McpResourceContent> = result
            .get("contents")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
            .unwrap_or_default();
        Ok(contents)
    }

    /// Close the connection
    pub async fn close(&mut self) -> Result<()> {
        self.transport.close().await
    }
}
```

- [ ] **Step 2: Commit**

---

### Task 4: MCP tool wrapper + connection manager

**Files:**
- Create: `src/services/mcp/tool_wrapper.rs`
- Create: `src/services/mcp/connection_manager.rs`

- [ ] **Step 1: Implement McpToolWrapper**

```rust
// src/services/mcp/tool_wrapper.rs
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::client::McpClient;
use super::types::McpToolDefinition;
use crate::tool::{Tool, ToolUseContext, ToolResult};
use crate::types::permissions::PermissionMeta;

/// Wraps an MCP tool to implement the Tool trait
pub struct McpToolWrapper {
    pub server_name: String,
    pub definition: McpToolDefinition,
    client: Arc<Mutex<McpClient>>,
}

impl McpToolWrapper {
    pub fn new(server_name: String, definition: McpToolDefinition, client: Arc<Mutex<McpClient>>) -> Self {
        Self { server_name, definition, client }
    }
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        self.definition.description.as_deref().unwrap_or("")
    }

    fn input_schema(&self) -> serde_json::Value {
        self.definition.input_schema.clone()
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let mut client = self.client.lock().await;
        let result = client.call_tool(&self.definition.name, ctx.input).await?;

        let content: String = result.content.iter().map(|c| match c {
            super::types::McpToolResultContent::Text { text } => text.clone(),
            super::types::McpToolResultContent::Image { .. } => "[image]".into(),
        }).collect::<Vec<_>>().join("\n");

        Ok(ToolResult {
            content,
            is_error: result.is_error,
        })
    }

    fn is_mcp(&self) -> bool { true }

    fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta { file_path: None, command: None, url: None }
    }
}
```

- [ ] **Step 2: Implement McpConnectionManager**

```rust
// src/services/mcp/connection_manager.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use super::client::McpClient;
use super::tool_wrapper::McpToolWrapper;
use super::transport::StdioTransport;
use super::types::McpServerConfig;
use crate::tool::Tool;
use crate::types::Result;

pub struct McpConnectionManager {
    configs: HashMap<String, McpServerConfig>,
    clients: HashMap<String, Arc<Mutex<McpClient>>>,
}

impl McpConnectionManager {
    pub fn new(configs: HashMap<String, McpServerConfig>) -> Self {
        Self {
            configs,
            clients: HashMap::new(),
        }
    }

    /// Connect to all configured MCP servers
    pub async fn connect_all(&mut self) -> Vec<String> {
        let mut errors = Vec::new();
        let configs: Vec<(String, McpServerConfig)> = self.configs.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        for (name, config) in configs {
            match self.connect(&name, &config).await {
                Ok(()) => info!(server = %name, "MCP server connected"),
                Err(e) => {
                    warn!(server = %name, error = %e, "Failed to connect MCP server");
                    errors.push(format!("{}: {}", name, e));
                }
            }
        }
        errors
    }

    async fn connect(&mut self, name: &str, config: &McpServerConfig) -> Result<()> {
        let transport = StdioTransport::spawn(config).await?;
        let mut client = McpClient::new(Box::new(transport));
        client.initialize().await?;
        self.clients.insert(name.to_string(), Arc::new(Mutex::new(client)));
        Ok(())
    }

    /// Get all tools from all connected servers
    pub async fn get_all_tools(&self) -> Vec<Box<dyn Tool>> {
        let mut tools: Vec<Box<dyn Tool>> = Vec::new();

        for (name, client_arc) in &self.clients {
            let mut client = client_arc.lock().await;
            match client.list_tools().await {
                Ok(defs) => {
                    for def in defs {
                        tools.push(Box::new(McpToolWrapper::new(
                            name.clone(),
                            def,
                            client_arc.clone(),
                        )));
                    }
                }
                Err(e) => {
                    warn!(server = %name, error = %e, "Failed to list tools");
                }
            }
        }

        tools
    }

    /// Disconnect a specific server
    pub async fn disconnect(&mut self, name: &str) -> Result<()> {
        if let Some(client_arc) = self.clients.remove(name) {
            let mut client = client_arc.lock().await;
            client.close().await?;
        }
        Ok(())
    }

    /// Disconnect all servers
    pub async fn disconnect_all(&mut self) {
        let names: Vec<String> = self.clients.keys().cloned().collect();
        for name in names {
            let _ = self.disconnect(&name).await;
        }
    }

    /// Get connected server names
    pub fn connected_servers(&self) -> Vec<&str> {
        self.clients.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_manager() {
        let manager = McpConnectionManager::new(HashMap::new());
        assert!(manager.connected_servers().is_empty());
    }
}
```

- [ ] **Step 3: Update mod.rs exports**

```rust
// src/services/mcp/mod.rs
pub mod jsonrpc;
pub mod transport;
pub mod client;
pub mod types;
pub mod connection_manager;
pub mod tool_wrapper;

pub use client::McpClient;
pub use connection_manager::McpConnectionManager;
pub use tool_wrapper::McpToolWrapper;
pub use types::{McpServerConfig, McpToolDefinition};
```

- [ ] **Step 4: Verify and commit**

Run: `cargo test --workspace`
Run: `cargo clippy --workspace -- -D warnings`

```bash
git add src/services/mcp/
git commit -m "feat: implement MCP client with stdio transport, tool wrapper, connection manager"
```
