use std::path::PathBuf;
use std::sync::Arc;

use crate::types::permissions::PermissionMeta;
use crate::utils::abort_signal::AbortSignal;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterruptBehavior {
    /// Abort on Interrupt reason
    Cancel,
    /// Continue to completion on Interrupt reason
    Block,
}

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

    /// Whether this tool should have its schema deferred (not sent upfront)
    fn should_defer(&self) -> bool {
        false
    }

    /// Whether this tool should always be loaded (never deferred)
    fn always_load(&self) -> bool {
        false
    }

    /// Hint for ToolSearch to match this tool
    fn search_hint(&self) -> Option<&str> {
        None
    }

    fn interrupt_behavior(&self) -> InterruptBehavior {
        InterruptBehavior::Block
    }
}

/// Context passed to tool execution
pub struct ToolUseContext {
    pub input: serde_json::Value,
    pub abort_signal: AbortSignal,
    /// Channel for sending UI updates during execution
    pub progress_tx: tokio::sync::mpsc::UnboundedSender<ToolProgress>,
    // Shared state for sub-agent spawning (SkillTool, AgentTool)
    pub provider: Arc<dyn crate::services::api::provider::LlmProvider>,
    pub registry: Option<Arc<crate::services::api::registry::ProviderRegistry>>,
    pub tools: Arc<Vec<Box<dyn Tool>>>,
    pub commands: Arc<crate::commands::registry::CommandRegistry>,
    pub system_prompt: Vec<crate::services::api::types::SystemBlock>,
    pub cwd: PathBuf,
    pub permission_ctx: crate::types::permissions::ToolPermissionContext,
    pub permission_tx: tokio::sync::mpsc::UnboundedSender<crate::query::types::PermissionAsk>,
    pub event_tx: tokio::sync::mpsc::UnboundedSender<crate::query::types::QueryEvent>,
    pub tool_use_id: String,
    pub question_tx: tokio::sync::mpsc::UnboundedSender<crate::query::types::UserQuestionAsk>,
    pub model: String,
    pub app_state: Arc<tokio::sync::RwLock<crate::state::AppState>>,
    pub mcp_manager: Option<Arc<crate::services::mcp::connection_manager::McpConnectionManager>>,
    pub agent_depth: u32,
    /// Parent conversation messages (shared ownership to avoid deep clone)
    pub messages: std::sync::Arc<Vec<crate::types::message::ApiMessage>>,
    /// Source identifier for recursion guard (e.g., "agent:builtin:fork")
    pub query_source: Option<String>,
}

#[cfg(test)]
impl ToolUseContext {
    pub fn test(input: serde_json::Value) -> Self {
        let (progress_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (permission_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let (question_tx, _) = tokio::sync::mpsc::unbounded_channel();
        Self {
            input,
            abort_signal: AbortSignal::new(),
            progress_tx,
            provider: Arc::new(crate::services::api::provider::NoopProvider),
            registry: None,
            tools: Arc::new(Vec::new()),
            commands: Arc::new(crate::commands::registry::CommandRegistry::empty()),
            system_prompt: Vec::new(),
            cwd: PathBuf::from("/tmp"),
            permission_ctx: crate::types::permissions::ToolPermissionContext {
                mode: crate::types::permissions::PermissionMode::BypassPermissions,
                ..Default::default()
            },
            permission_tx,
            event_tx,
            tool_use_id: String::new(),
            question_tx,
            model: "test".into(),
            app_state: Arc::new(tokio::sync::RwLock::new(crate::state::AppState::default())),
            mcp_manager: None,
            agent_depth: 0,
            messages: std::sync::Arc::new(Vec::new()),
            query_source: None,
        }
    }
}

/// Result of tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<crate::types::content::ContentBlock>>,
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

fn is_false(v: &bool) -> bool {
    !v
}

/// Tool definition sent to LLM API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "is_false")]
    pub defer_loading: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<crate::types::content::ContentBlock>>,
}

impl ToolResultBlock {
    pub fn success(tool_use_id: String, content: String) -> Self {
        Self {
            tool_use_id,
            content,
            is_error: false,
            content_blocks: None,
        }
    }

    pub fn error(tool_use_id: String, message: String) -> Self {
        Self {
            tool_use_id,
            content: message,
            is_error: true,
            content_blocks: None,
        }
    }
}

impl From<&dyn Tool> for ToolDefinition {
    fn from(tool: &dyn Tool) -> Self {
        ToolDefinition {
            name: tool.name().to_string(),
            description: tool.description().to_string(),
            input_schema: tool.input_schema(),
            defer_loading: crate::tools::is_deferred_tool(tool),
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
            defer_loading: false,
        };
        let json = serde_json::to_value(&def).unwrap();
        assert_eq!(json["name"], "BashTool");
        assert_eq!(json["input_schema"]["type"], "object");
        // defer_loading=false should be omitted from serialization
        assert!(json.get("defer_loading").is_none());
    }

    #[test]
    fn test_tool_definition_defer_loading_true() {
        let def = ToolDefinition {
            name: "SomeTool".into(),
            description: "A deferred tool".into(),
            input_schema: serde_json::json!({"type": "object"}),
            defer_loading: true,
        };
        let json = serde_json::to_value(&def).unwrap();
        assert_eq!(json["defer_loading"], true);
    }

    #[test]
    fn test_tool_definition_defer_loading_false_omitted() {
        let def = ToolDefinition {
            name: "SomeTool".into(),
            description: "A non-deferred tool".into(),
            input_schema: serde_json::json!({"type": "object"}),
            defer_loading: false,
        };
        let json = serde_json::to_value(&def).unwrap();
        assert!(
            json.get("defer_loading").is_none(),
            "defer_loading=false should be omitted from JSON"
        );
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
