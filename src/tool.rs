use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::types::permissions::PermissionMeta;

/// Core tool trait — all 52 built-in tools + MCP tools implement this
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name as sent to the LLM
    fn name(&self) -> &str;

    /// Tool description for LLM context
    fn description(&self) -> &str;

    /// JSON schema for tool input
    fn input_schema(&self) -> serde_json::Value;

    /// Execute the tool with given context
    async fn execute(&self, ctx: ToolUseContext) -> crate::types::Result<ToolResult>;

    /// Check if the tool can be used in current context
    fn can_use(&self, _ctx: &ToolUseContext) -> ValidationResult {
        ValidationResult::Ok
    }

    /// Permission metadata for this tool invocation
    fn permission_metadata(&self, input: &serde_json::Value) -> PermissionMeta {
        let _ = input;
        PermissionMeta {
            file_path: None,
            command: None,
            url: None,
        }
    }

    /// Whether the tool is read-only (no side effects)
    fn is_read_only(&self) -> bool {
        false
    }

    /// Whether tool calls can run concurrently
    fn is_concurrent_safe(&self) -> bool {
        false
    }

    /// Aliases for tool search
    fn aliases(&self) -> &[&str] {
        &[]
    }

    /// Whether this is an MCP tool
    fn is_mcp(&self) -> bool {
        false
    }
}

/// Context passed to tool execution
pub struct ToolUseContext {
    pub input: serde_json::Value,
    pub abort_signal: CancellationToken,
    /// Channel for sending UI updates during execution
    pub progress_tx: tokio::sync::mpsc::UnboundedSender<ToolProgress>,
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

/// Progress update from a running tool
#[derive(Debug, Clone)]
pub enum ToolProgress {
    Output(String),
    Partial(String),
    Complete,
}

/// Validation result for can_use check
#[derive(Debug)]
pub enum ValidationResult {
    Ok,
    Disabled(String),
    Error(String),
}

/// Tool definition sent to LLM API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// A tool call extracted from assistant response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

/// Tool result block sent back to LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

impl ToolResultBlock {
    pub fn success(tool_use_id: String, content: String) -> Self {
        Self {
            tool_use_id,
            content,
            is_error: false,
        }
    }

    pub fn error(tool_use_id: String, message: String) -> Self {
        Self {
            tool_use_id,
            content: message,
            is_error: true,
        }
    }
}

impl From<&dyn Tool> for ToolDefinition {
    fn from(tool: &dyn Tool) -> Self {
        ToolDefinition {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_result_block_success() {
        let block = ToolResultBlock::success("tu_1".into(), "output".into());
        assert!(!block.is_error);
        assert_eq!(block.content, "output");
    }

    #[test]
    fn test_tool_result_block_error() {
        let block = ToolResultBlock::error("tu_1".into(), "failed".into());
        assert!(block.is_error);
        assert_eq!(block.content, "failed");
    }

    #[test]
    fn test_tool_definition_serde() {
        let def = ToolDefinition {
            name: "BashTool".into(),
            description: "Run bash commands".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                },
                "required": ["command"]
            }),
        };
        let json = serde_json::to_value(&def).unwrap();
        assert_eq!(json["name"], "BashTool");
        assert_eq!(json["input_schema"]["type"], "object");
    }

    #[test]
    fn test_tool_call_serde() {
        let call = ToolCall {
            id: "tu_abc123".into(),
            name: "FileReadTool".into(),
            input: serde_json::json!({"file_path": "/tmp/test.rs"}),
        };
        let json = serde_json::to_string(&call).unwrap();
        let parsed: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "FileReadTool");
    }
}
