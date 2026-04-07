use async_trait::async_trait;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::tools::is_deferred_tool;
use crate::types::content::ContentBlock;

pub struct ToolSearchTool;

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "ToolSearch"
    }

    fn description(&self) -> &str {
        "Fetches full schema definitions for deferred tools so they can be called."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Query to find deferred tools. Use 'select:<tool_name>' for direct selection, or keywords to search."
                },
                "max_results": {
                    "type": "number",
                    "default": 5,
                    "description": "Maximum number of results to return (default: 5)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let query = ctx
            .input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "ToolSearch".into(),
                message: "Missing required 'query' parameter".into(),
            })?;

        let max_results = ctx
            .input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize;

        let tools = &*ctx.tools;

        // Mode 1: select:Name1,Name2
        if let Some(names_str) = query.strip_prefix("select:") {
            let names: Vec<&str> = names_str.split(',').map(|s| s.trim()).collect();
            let matched = find_by_exact_names(tools, &names);
            return Ok(build_result(query, tools, &matched));
        }

        // Mode 2: Exact name fast path (case-insensitive)
        let query_lower = query.to_lowercase();
        if let Some(tool) = tools.iter().find(|t| t.name().to_lowercase() == query_lower) {
            return Ok(build_result(query, tools, &[tool.name().to_string()]));
        }

        // Mode 3: mcp__ prefix match
        if query.starts_with("mcp__") {
            let matched: Vec<String> = tools
                .iter()
                .filter(|t| t.name().starts_with(query))
                .map(|t| t.name().to_string())
                .take(max_results)
                .collect();
            return Ok(build_result(query, tools, &matched));
        }

        // Modes 4 & 5: keyword search on deferred tools only
        let deferred: Vec<&dyn Tool> = tools
            .iter()
            .filter(|t| is_deferred_tool(t.as_ref()))
            .map(|t| t.as_ref())
            .collect();

        let (required_terms, optional_terms) = parse_keyword_query(query);

        let mut scored: Vec<(String, i32)> = Vec::new();
        for tool in &deferred {
            let score = score_tool(*tool, &required_terms, &optional_terms);
            if score > 0 {
                // For mode 4: all required terms must match
                if !required_terms.is_empty() {
                    let all_required = required_terms.iter().all(|req| {
                        term_matches_tool(*tool, req)
                    });
                    if !all_required {
                        continue;
                    }
                }
                scored.push((tool.name().to_string(), score));
            }
        }

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.truncate(max_results);

        let matched: Vec<String> = scored.into_iter().map(|(name, _)| name).collect();
        Ok(build_result(query, tools, &matched))
    }

    fn is_read_only(&self) -> bool {
        true
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }

    fn always_load(&self) -> bool {
        true
    }
}

fn find_by_exact_names(tools: &[Box<dyn Tool>], names: &[&str]) -> Vec<String> {
    let mut matched = Vec::new();
    for name in names {
        if let Some(tool) = tools.iter().find(|t| t.name().eq_ignore_ascii_case(name)) {
            matched.push(tool.name().to_string());
        }
    }
    matched
}

fn parse_keyword_query(query: &str) -> (Vec<String>, Vec<String>) {
    let mut required = Vec::new();
    let mut optional = Vec::new();

    for term in query.split_whitespace() {
        if let Some(req) = term.strip_prefix('+') {
            if !req.is_empty() {
                required.push(req.to_lowercase());
            }
        } else {
            optional.push(term.to_lowercase());
        }
    }

    (required, optional)
}

/// Check if a single term matches anywhere in a tool's searchable fields.
fn term_matches_tool(tool: &dyn Tool, term: &str) -> bool {
    let name_lower = tool.name().to_lowercase();
    let desc_lower = tool.description().to_lowercase();
    let hint_lower = tool.search_hint().unwrap_or("").to_lowercase();

    name_lower.contains(term) || desc_lower.contains(term) || hint_lower.contains(term)
}

fn score_tool(tool: &dyn Tool, required: &[String], optional: &[String]) -> i32 {
    let name_lower = tool.name().to_lowercase();
    let desc_lower = tool.description().to_lowercase();
    let hint_lower = tool.search_hint().unwrap_or("").to_lowercase();
    let is_mcp = tool.is_mcp();

    let all_terms: Vec<&String> = required.iter().chain(optional.iter()).collect();
    if all_terms.is_empty() {
        return 0;
    }

    let mut total = 0i32;

    for term in &all_terms {
        // Name scoring
        let full_word_name = name_lower
            .split(['_', '-'])
            .any(|segment| segment == term.as_str());
        if full_word_name {
            total += if is_mcp { 12 } else { 10 };
        } else if name_lower.contains(term.as_str()) {
            total += if is_mcp { 6 } else { 5 };
        }

        // search_hint scoring (full word only)
        if !hint_lower.is_empty() {
            let full_word_hint = hint_lower
                .split_whitespace()
                .any(|w| w == term.as_str());
            if full_word_hint {
                total += 4;
            }
        }

        // description scoring (full word only)
        let full_word_desc = desc_lower
            .split_whitespace()
            .any(|w| w == term.as_str());
        if full_word_desc {
            total += 2;
        }
    }

    total
}

fn build_result(query: &str, tools: &[Box<dyn Tool>], matched_names: &[String]) -> ToolResult {
    let total_deferred = tools.iter().filter(|t| is_deferred_tool(t.as_ref())).count();

    let mut blocks: Vec<ContentBlock> = Vec::new();

    for name in matched_names {
        blocks.push(ContentBlock::ToolReference {
            tool_name: name.clone(),
        });
    }

    let metadata = serde_json::json!({
        "query": query,
        "total_deferred_tools": total_deferred,
        "matches": matched_names,
    });
    blocks.push(ContentBlock::Text {
        text: metadata.to_string(),
    });

    ToolResult {
        content: format!("Found {} tool(s) matching '{}'", matched_names.len(), query),
        is_error: false,
        content_blocks: Some(blocks),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_ctx(input: serde_json::Value) -> ToolUseContext {
        let mut ctx = ToolUseContext::test(input);
        ctx.tools = Arc::new(crate::tools::get_builtin_tools());
        ctx
    }

    #[test]
    fn test_name() {
        assert_eq!(ToolSearchTool.name(), "ToolSearch");
    }

    #[test]
    fn test_schema() {
        let schema = ToolSearchTool.input_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("query").is_some());
        assert!(props.get("max_results").is_some());
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert!(required.contains(&serde_json::json!("query")));
        assert!(!required.contains(&serde_json::json!("max_results")));
    }

    #[test]
    fn test_always_load() {
        assert!(ToolSearchTool.always_load());
        assert!(ToolSearchTool.is_read_only());
        assert!(ToolSearchTool.is_concurrent_safe());
    }

    #[tokio::test]
    async fn test_select_mode() {
        let ctx = make_ctx(serde_json::json!({"query": "select:Bash,Read"}));
        let result = ToolSearchTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let blocks = result.content_blocks.unwrap();
        // Should have ToolReference for Bash, ToolReference for Read, and metadata Text
        let refs: Vec<&ContentBlock> = blocks
            .iter()
            .filter(|b| matches!(b, ContentBlock::ToolReference { .. }))
            .collect();
        assert_eq!(refs.len(), 2);

        assert!(matches!(&refs[0], ContentBlock::ToolReference { tool_name } if tool_name == "Bash"));
        assert!(matches!(&refs[1], ContentBlock::ToolReference { tool_name } if tool_name == "Read"));
    }

    #[tokio::test]
    async fn test_exact_name_match() {
        let ctx = make_ctx(serde_json::json!({"query": "Bash"}));
        let result = ToolSearchTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let blocks = result.content_blocks.unwrap();
        let refs: Vec<&ContentBlock> = blocks
            .iter()
            .filter(|b| matches!(b, ContentBlock::ToolReference { .. }))
            .collect();
        assert_eq!(refs.len(), 1);
        assert!(matches!(&refs[0], ContentBlock::ToolReference { tool_name } if tool_name == "Bash"));
    }

    #[tokio::test]
    async fn test_exact_name_case_insensitive() {
        let ctx = make_ctx(serde_json::json!({"query": "bash"}));
        let result = ToolSearchTool.execute(ctx).await.unwrap();
        assert!(!result.is_error);

        let blocks = result.content_blocks.unwrap();
        let refs: Vec<&ContentBlock> = blocks
            .iter()
            .filter(|b| matches!(b, ContentBlock::ToolReference { .. }))
            .collect();
        assert_eq!(refs.len(), 1);
        assert!(matches!(&refs[0], ContentBlock::ToolReference { tool_name } if tool_name == "Bash"));
    }

    #[tokio::test]
    async fn test_missing_query() {
        let ctx = make_ctx(serde_json::json!({}));
        let result = ToolSearchTool.execute(ctx).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_scoring_name_full_word() {
        // A mock tool that tests scoring
        struct MockTool;

        #[async_trait]
        impl Tool for MockTool {
            fn name(&self) -> &str { "web_search" }
            fn description(&self) -> &str { "Search the web" }
            fn input_schema(&self) -> serde_json::Value { serde_json::json!({"type": "object"}) }
            async fn execute(&self, _ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
                unimplemented!()
            }
        }

        // "search" is a full word in name segments split by '_'
        let score = score_tool(&MockTool, &[], &["search".into()]);
        // name full word (non-MCP) = 10, description full word "search" = 0 (not a standalone word in "Search the web")
        // Actually "Search" lowered in desc: split_whitespace gives ["search", "the", "web"], "search" matches => +2
        assert_eq!(score, 10 + 2);
    }

    #[test]
    fn test_scoring_mcp_bonus() {
        struct MockMcpTool;

        #[async_trait]
        impl Tool for MockMcpTool {
            fn name(&self) -> &str { "mcp__server__search" }
            fn description(&self) -> &str { "Search tool" }
            fn input_schema(&self) -> serde_json::Value { serde_json::json!({"type": "object"}) }
            async fn execute(&self, _ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
                unimplemented!()
            }
            fn is_mcp(&self) -> bool { true }
        }

        let score = score_tool(&MockMcpTool, &[], &["search".into()]);
        // name full word (MCP) = 12, description full word "search" not a standalone word...
        // desc is "Search tool" => ["search", "tool"], "search" matches => +2
        assert_eq!(score, 12 + 2);
    }

    #[test]
    fn test_parse_keyword_query() {
        let (req, opt) = parse_keyword_query("+slack send message");
        assert_eq!(req, vec!["slack"]);
        assert_eq!(opt, vec!["send", "message"]);
    }

    #[test]
    fn test_parse_keyword_query_no_required() {
        let (req, opt) = parse_keyword_query("notebook jupyter");
        assert!(req.is_empty());
        assert_eq!(opt, vec!["notebook", "jupyter"]);
    }
}
