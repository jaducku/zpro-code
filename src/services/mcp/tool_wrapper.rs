use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::client::McpClient;
use super::types::McpToolDefinition;
use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

/// Wraps an MCP tool to implement the Tool trait
pub struct McpToolWrapper {
    pub server_name: String,
    pub definition: McpToolDefinition,
    client: Arc<Mutex<McpClient>>,
}

impl McpToolWrapper {
    pub fn new(
        server_name: String,
        definition: McpToolDefinition,
        client: Arc<Mutex<McpClient>>,
    ) -> Self {
        Self {
            server_name,
            definition,
            client,
        }
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

        let content: String = result
            .content
            .iter()
            .map(|c| match c {
                super::types::McpToolResultContent::Text { text } => text.clone(),
                super::types::McpToolResultContent::Image { .. } => "[image]".into(),
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult {
            content,
            is_error: result.is_error,
            content_blocks: None,
        })
    }

    fn is_mcp(&self) -> bool {
        true
    }

    fn permission_metadata(&self, _input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            file_path: None,
            command: None,
            url: None,
        }
    }
}
