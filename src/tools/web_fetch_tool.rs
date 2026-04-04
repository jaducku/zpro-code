use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::permissions::PermissionMeta;

pub struct WebFetchTool;

const MAX_BODY_SIZE: usize = 5 * 1024 * 1024; // 5 MB
const MAX_CONTENT_CHARS: usize = 20_000;

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "WebFetch"
    }

    fn description(&self) -> &str {
        "Fetch a URL and return its content as markdown. Useful for reading documentation, articles, and web pages."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch (must start with http:// or https://)"
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional instruction for what to extract from the page"
                }
            },
            "required": ["url"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let url_str = ctx
            .input
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "WebFetch".into(),
                message: "Missing 'url' parameter".into(),
            })?;

        let prompt = ctx.input.get("prompt").and_then(|v| v.as_str());

        // Validate URL scheme
        if !url_str.starts_with("http://") && !url_str.starts_with("https://") {
            return Ok(ToolResult {
                content: "Invalid URL: must start with http:// or https://".into(),
                is_error: true,
                content_blocks: None,
            });
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("atom-code/0.1")
            .build()
            .map_err(|e| crate::types::AtomError::Tool {
                tool_name: "WebFetch".into(),
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        let response = tokio::select! {
            result = client.get(url_str).send() => {
                match result {
                    Ok(resp) => resp,
                    Err(e) => {
                        return Ok(ToolResult {
                            content: format!("Fetch failed: {}", e),
                            is_error: true,
                            content_blocks: None,
                        });
                    }
                }
            }
            _ = ctx.abort_signal.cancelled() => {
                return Err(crate::types::AtomError::Cancelled);
            }
        };

        let status = response.status();
        if status != 200 {
            return Ok(ToolResult {
                content: format!("HTTP error: status {}", status),
                is_error: true,
                content_blocks: None,
            });
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let is_html = content_type.contains("text/html") || content_type.contains("application/xhtml");
        let is_text = content_type.contains("text/") || content_type.contains("application/json")
            || content_type.contains("application/xml");

        if !is_html && !is_text {
            return Ok(ToolResult {
                content: format!("Cannot process binary content type: {}", content_type),
                is_error: true,
                content_blocks: None,
            });
        }

        let bytes = tokio::select! {
            result = response.bytes() => {
                match result {
                    Ok(b) => b,
                    Err(e) => {
                        return Ok(ToolResult {
                            content: format!("Failed to read response body: {}", e),
                            is_error: true,
                            content_blocks: None,
                        });
                    }
                }
            }
            _ = ctx.abort_signal.cancelled() => {
                return Err(crate::types::AtomError::Cancelled);
            }
        };

        if bytes.len() > MAX_BODY_SIZE {
            return Ok(ToolResult {
                content: format!("Response too large: {} bytes (max {})", bytes.len(), MAX_BODY_SIZE),
                is_error: true,
                content_blocks: None,
            });
        }

        let mut content = if is_html {
            html_to_markdown(url_str, &bytes)
        } else {
            String::from_utf8_lossy(&bytes).to_string()
        };

        // Truncate to max chars
        if content.len() > MAX_CONTENT_CHARS {
            content.truncate(MAX_CONTENT_CHARS);
            content.push_str("\n\n[Content truncated]");
        }

        // Prepend prompt if provided
        if let Some(prompt) = prompt {
            content = format!("Instructions: {}\n\n---\n\n{}", prompt, content);
        }

        Ok(ToolResult {
            content,
            is_error: false,
            content_blocks: None,
        })
    }

    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        PermissionMeta {
            url: input.get("url").and_then(|v| v.as_str()).map(String::from),
            file_path: None,
            command: None,
        }
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }
}

fn html_to_markdown(url_str: &str, html_bytes: &[u8]) -> String {
    // Try readability first for main content extraction
    let mut cursor = std::io::Cursor::new(html_bytes);
    let readability_result = url::Url::parse(url_str)
        .ok()
        .and_then(|parsed_url| readability::extractor::extract(&mut cursor, &parsed_url).ok());

    if let Some(product) = readability_result {
        let body_md =
            htmd::convert(&product.content).unwrap_or(product.text);
        if !product.title.is_empty() {
            format!("# {}\n\n{}", product.title, body_md)
        } else {
            body_md
        }
    } else {
        // Fallback: convert raw HTML directly
        let raw_html = String::from_utf8_lossy(html_bytes);
        htmd::convert(&raw_html).unwrap_or_else(|_| raw_html.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(WebFetchTool.name(), "WebFetch");
    }

    #[test]
    fn test_schema_has_required_url() {
        let schema = WebFetchTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["url"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("url")));
    }

    #[tokio::test]
    async fn test_invalid_url_scheme() {
        let tool = WebFetchTool;
        let ctx = ToolUseContext::test(serde_json::json!({"url": "ftp://example.com"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("Invalid URL"));
    }

    #[test]
    fn test_html_to_markdown() {
        let html = b"<html><head><title>Hello</title></head><body><h1>Hello</h1><p>World</p></body></html>";
        let md = html_to_markdown("https://example.com", html);
        assert!(md.contains("Hello"));
        assert!(md.contains("World"));
    }

    #[test]
    fn test_permission_metadata_extracts_url() {
        let tool = WebFetchTool;
        let meta = tool.permission_metadata(&serde_json::json!({"url": "https://example.com"}));
        assert_eq!(meta.url.as_deref(), Some("https://example.com"));
        assert!(meta.file_path.is_none());
        assert!(meta.command.is_none());
    }
}
