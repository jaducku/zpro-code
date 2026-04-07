use async_trait::async_trait;
use serde::Deserialize;

use crate::tool::{Tool, ToolResult, ToolUseContext};

pub struct WebSearchTool;

#[derive(Deserialize)]
struct TavilyResponse {
    results: Vec<TavilyResult>,
}

#[derive(Deserialize)]
struct TavilyResult {
    title: String,
    url: String,
    content: String,
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "WebSearch"
    }

    fn description(&self) -> &str {
        "Search the web using Tavily and return relevant results with content."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                },
                "num_results": {
                    "type": "integer",
                    "description": "Number of results to return (default 5, max 10)",
                    "default": 5,
                    "maximum": 10
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let query = match ctx.input.get("query").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => {
                return Ok(ToolResult {
                    content: "Missing required parameter: 'query'".into(),
                    is_error: true,
                    content_blocks: None,
                });
            }
        };

        let num_results = ctx
            .input
            .get("num_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5)
            .min(10) as usize;

        let api_key = match crate::services::config::AtomConfig::load().tavily_api_key {
            Some(k) => k,
            None => {
                return Ok(ToolResult {
                    content: "Tavily API key not configured. Add \"tavily_api_key\": \"tvly-...\" to ~/.zpro-code/config.json".into(),
                    is_error: true,
                    content_blocks: None,
                });
            }
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| crate::types::AtomError::Tool {
                tool_name: "WebSearch".into(),
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        let body = serde_json::json!({
            "query": query,
            "max_results": num_results,
            "include_answer": false,
            "include_raw_content": false
        });

        let response = tokio::select! {
            result = client
                .post("https://api.tavily.com/search")
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&body)
                .send() =>
            {
                match result {
                    Ok(resp) => resp,
                    Err(e) => {
                        return Ok(ToolResult {
                            content: format!("Search request failed: {}", e),
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
        if !status.is_success() {
            return Ok(ToolResult {
                content: format!("Tavily API error: HTTP {}", status),
                is_error: true,
                content_blocks: None,
            });
        }

        let tavily_response: TavilyResponse = tokio::select! {
            result = response.json() => {
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        return Ok(ToolResult {
                            content: format!("Failed to parse Tavily response: {}", e),
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

        if tavily_response.results.is_empty() {
            return Ok(ToolResult {
                content: format!("No results found for '{}'", query),
                is_error: false,
                content_blocks: None,
            });
        }

        let formatted = tavily_response
            .results
            .iter()
            .map(|r| format!("### [{}]({})\n\n{}", r.title, r.url, r.content))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        Ok(ToolResult {
            content: formatted,
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

    fn interrupt_behavior(&self) -> crate::tool::InterruptBehavior {
        crate::tool::InterruptBehavior::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name() {
        assert_eq!(WebSearchTool.name(), "WebSearch");
    }

    #[test]
    fn test_schema_has_required_query() {
        let schema = WebSearchTool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["query"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("query")));
    }

    #[tokio::test]
    async fn test_missing_api_key() {
        // In test env there is no ~/.zpro-code/config.json with tavily_api_key
        let tool = WebSearchTool;
        let ctx = ToolUseContext::test(serde_json::json!({"query": "rust programming"}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("not configured"));
    }

    #[tokio::test]
    async fn test_missing_query_param() {
        let tool = WebSearchTool;
        let ctx = ToolUseContext::test(serde_json::json!({}));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(result.content.contains("query"));
    }
}
