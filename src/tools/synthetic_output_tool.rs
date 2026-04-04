use std::sync::Arc;

use async_trait::async_trait;
use jsonschema::Validator;

use crate::tool::{Tool, ToolResult, ToolUseContext};
use crate::types::AtomError;

pub struct SyntheticOutputTool {
    schema: serde_json::Value,
    compiled: Arc<Validator>,
}

impl SyntheticOutputTool {
    pub fn new(json_schema: serde_json::Value) -> Result<Self, AtomError> {
        let compiled = jsonschema::validator_for(&json_schema).map_err(|e| AtomError::Tool {
            tool_name: "StructuredOutput".into(),
            message: format!("Invalid JSON schema: {}", e),
        })?;
        Ok(Self {
            schema: json_schema,
            compiled: Arc::new(compiled),
        })
    }
}

#[async_trait]
impl Tool for SyntheticOutputTool {
    fn name(&self) -> &str {
        "StructuredOutput"
    }

    fn description(&self) -> &str {
        "Validate and return structured JSON output against a caller-provided schema"
    }

    fn input_schema(&self) -> serde_json::Value {
        self.schema.clone()
    }

    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult> {
        let errors: Vec<String> = self
            .compiled
            .iter_errors(&ctx.input)
            .map(|e| e.to_string())
            .collect();

        if !errors.is_empty() {
            return Ok(ToolResult {
                content: format!("Validation errors:\n{}", errors.join("\n")),
                is_error: true,
                content_blocks: None,
            });
        }

        let output = serde_json::json!({
            "data": "Structured output provided successfully",
            "structured_output": ctx.input
        });

        Ok(ToolResult {
            content: output.to_string(),
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

    fn search_hint(&self) -> Option<&str> {
        Some("return the final response as structured JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name"]
        })
    }

    #[test]
    fn test_name() {
        let tool = SyntheticOutputTool::new(test_schema()).unwrap();
        assert_eq!(tool.name(), "StructuredOutput");
    }

    #[test]
    fn test_input_schema_returns_user_schema() {
        let schema = test_schema();
        let tool = SyntheticOutputTool::new(schema.clone()).unwrap();
        assert_eq!(tool.input_schema(), schema);
    }

    #[test]
    fn test_properties() {
        let tool = SyntheticOutputTool::new(test_schema()).unwrap();
        assert!(tool.is_read_only());
        assert!(tool.is_concurrent_safe());
    }

    #[tokio::test]
    async fn test_valid_input() {
        let tool = SyntheticOutputTool::new(test_schema()).unwrap();
        let ctx = ToolUseContext::test(serde_json::json!({ "name": "Alice", "age": 30 }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(!result.is_error);
        let parsed: serde_json::Value = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed["data"], "Structured output provided successfully");
        assert_eq!(parsed["structured_output"]["name"], "Alice");
        assert_eq!(parsed["structured_output"]["age"], 30);
    }

    #[tokio::test]
    async fn test_invalid_input_missing_required() {
        let tool = SyntheticOutputTool::new(test_schema()).unwrap();
        let ctx = ToolUseContext::test(serde_json::json!({ "age": 30 }));
        let result = tool.execute(ctx).await.unwrap();
        assert!(result.is_error);
        assert!(
            result.content.contains("name"),
            "Error should mention missing 'name' field, got: {}",
            result.content
        );
    }
}
