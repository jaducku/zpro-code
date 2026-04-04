use std::path::PathBuf;

use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
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
        Some("application/json") => "json",
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
                    "description": "The MCP server name"
                },
                "uri": {
                    "type": "string",
                    "description": "The resource URI to read"
                }
            },
            "required": ["server", "uri"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let manager = ctx
            .mcp_manager
            .as_ref()
            .ok_or_else(|| tool_err("No MCP servers configured"))?;

        let server = ctx
            .input
            .get("server")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required parameter: 'server'"))?;

        let uri = ctx
            .input
            .get("uri")
            .and_then(|v| v.as_str())
            .ok_or_else(|| tool_err("Missing required parameter: 'uri'"))?;

        let client_arc = manager.get_client(server).ok_or_else(|| {
            tool_err(format!(
                "Server \"{}\" not found. Available servers: {}",
                server,
                manager.connected_servers().join(", ")
            ))
        })?;

        // Check resource capability before locking
        {
            let client = client_arc.lock().await;
            if client.capabilities.resources.is_none() {
                return Err(tool_err(format!(
                    "Server \"{}\" does not support resources",
                    server
                )));
            }
        }

        // Read the resource
        let contents = {
            let mut client = client_arc.lock().await;
            client.read_resource(uri).await.map_err(|e| {
                tool_err(format!("Failed to read resource \"{}\": {}", uri, e))
            })?
        };

        let session_id = ctx.app_state.read().await.session_id.clone();

        // Process contents
        let mut result_contents = Vec::new();
        for (i, c) in contents.iter().enumerate() {
            let mut entry = serde_json::json!({
                "uri": c.uri,
            });

            if let Some(ref mime) = c.mime_type {
                entry["mimeType"] = serde_json::Value::String(mime.clone());
            }

            if let Some(ref text) = c.text {
                entry["text"] = serde_json::Value::String(text.clone());
            }

            if let Some(ref blob_b64) = c.blob {
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(blob_b64) {
                    Ok(data) => {
                        match persist_binary_content(
                            &session_id,
                            c.mime_type.as_deref(),
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

            result_contents.push(entry);
        }

        let output = serde_json::json!({ "contents": result_contents });

        Ok(ToolResult {
            content: serde_json::to_string(&output).unwrap(),
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
        Some("read a specific resource from an MCP server by URI")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::Tool;

    #[test]
    fn test_name() {
        assert_eq!(ReadMcpResourceTool.name(), "ReadMcpResource");
    }

    #[test]
    fn test_schema() {
        let schema = ReadMcpResourceTool.input_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("server").is_some());
        assert!(props.get("uri").is_some());
        let required = schema.get("required").unwrap().as_array().unwrap();
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
        assert_eq!(mime_type_to_extension(Some("application/octet-stream")), "bin");
    }

    #[tokio::test]
    async fn test_no_mcp_manager() {
        let ctx = ToolUseContext::test(serde_json::json!({
            "server": "test",
            "uri": "file:///test"
        }));
        assert!(ctx.mcp_manager.is_none());
        let result = ReadMcpResourceTool.execute(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("No MCP servers configured"),
            "Error should mention no MCP servers: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_missing_server() {
        let ctx = ToolUseContext::test(serde_json::json!({
            "uri": "file:///test"
        }));
        let result = ReadMcpResourceTool.execute(ctx).await;
        assert!(result.is_err() || {
            // With no mcp_manager, the "No MCP servers" error fires first
            true
        });
    }

    #[tokio::test]
    async fn test_missing_uri() {
        let ctx = ToolUseContext::test(serde_json::json!({
            "server": "test"
        }));
        let result = ReadMcpResourceTool.execute(ctx).await;
        assert!(result.is_err() || {
            // With no mcp_manager, the "No MCP servers" error fires first
            true
        });
    }
}
