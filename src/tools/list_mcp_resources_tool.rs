use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
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
                    "description": "Optional server name to filter resources by"
                }
            }
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let manager = ctx
            .mcp_manager
            .as_ref()
            .ok_or_else(|| tool_err("No MCP servers configured"))?;

        let server = ctx.input.get("server").and_then(|v| v.as_str());

        // Build list of (server_name, client_arc) to process
        let clients: Vec<(String, _)> = if let Some(server_name) = server {
            let client = manager
                .get_client(server_name)
                .ok_or_else(|| {
                    tool_err(format!(
                        "Server \"{}\" not found. Available servers: {}",
                        server_name,
                        manager.connected_servers().join(", ")
                    ))
                })?;
            vec![(server_name.to_string(), client)]
        } else {
            manager.get_all_clients()
        };

        let mut resources = Vec::new();

        for (server_name, client_arc) in clients {
            let mut client = client_arc.lock().await;

            // Skip servers that don't advertise resource support
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
                    tracing::warn!(
                        server = %server_name,
                        error = %e,
                        "Failed to list resources"
                    );
                }
            }
        }

        if resources.is_empty() {
            return Ok(ToolResult {
                content: "No resources found. MCP servers may still provide tools even if they have no resources.".into(),
                is_error: false,
                content_blocks: None,
            });
        }

        Ok(ToolResult {
            content: serde_json::to_string(&resources).unwrap(),
            is_error: false,
            content_blocks: None,
        })
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    fn should_defer(&self) -> bool {
        true
    }

    fn search_hint(&self) -> Option<&str> {
        Some("list resources from connected MCP servers")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;

    #[test]
    fn test_name() {
        assert_eq!(ListMcpResourcesTool.name(), "ListMcpResources");
    }

    #[test]
    fn test_schema() {
        let schema = ListMcpResourcesTool.input_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("server").is_some());
    }

    #[test]
    fn test_properties() {
        assert!(ListMcpResourcesTool.is_read_only());
        assert!(ListMcpResourcesTool.is_concurrent_safe());
        assert!(ListMcpResourcesTool.should_defer());
    }

    #[tokio::test]
    async fn test_no_mcp_manager() {
        let ctx = ToolUseContext::test(serde_json::json!({}));
        assert!(ctx.mcp_manager.is_none());
        let result = ListMcpResourcesTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("No MCP servers configured"),
            "Error should mention no MCP servers: {}",
            err
        );
    }
}
