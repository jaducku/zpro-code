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
    async fn request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
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
                            server_name: self
                                .server_info
                                .as_ref()
                                .map(|s| s.name.clone())
                                .unwrap_or_default(),
                            message: format!(
                                "JSON-RPC error {}: {}",
                                error.code, error.message
                            ),
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
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolResult> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments
        });
        let result = self.request("tools/call", Some(params)).await?;
        let tool_result: McpToolResult =
            serde_json::from_value(result).map_err(|e| crate::types::AtomError::Mcp {
                server_name: self
                    .server_info
                    .as_ref()
                    .map(|s| s.name.clone())
                    .unwrap_or_default(),
                message: format!("Failed to parse tool result: {}", e),
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
