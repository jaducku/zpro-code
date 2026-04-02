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
        let configs: Vec<(String, McpServerConfig)> = self
            .configs
            .iter()
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
        self.clients
            .insert(name.to_string(), Arc::new(Mutex::new(client)));
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
