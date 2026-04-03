use std::path::PathBuf;

use tokio::sync::mpsc;

use crate::services::api::types::SystemBlock;
use crate::services::hooks::HookConfig;
use crate::tool::{Tool, ToolResultBlock};
use crate::types::message::StopReason;
use crate::types::permissions::ToolPermissionContext;

/// Context for a query execution
pub struct QueryContext {
    pub messages: Vec<crate::types::message::ApiMessage>,
    pub tools: Vec<Box<dyn Tool>>,
    pub system_prompt: Option<Vec<SystemBlock>>,
    pub model: String,
    pub max_tokens: u32,
    pub permission_ctx: ToolPermissionContext,
    pub permission_tx: mpsc::UnboundedSender<PermissionAsk>,
    pub hooks: Vec<HookConfig>,
    pub cwd: PathBuf,
}

/// Sent to the TUI when a tool needs user permission
pub struct PermissionAsk {
    pub tool_use_id: String,
    pub tool_name: String,
    pub description: String,
    pub reply_tx: tokio::sync::oneshot::Sender<PermissionReply>,
}

/// User's response to a permission prompt
pub enum PermissionReply {
    Allow,
    AllowAlways {
        tool_name: String,
        rule_content: Option<String>,
    },
    Deny,
}

/// Result of a query execution
pub struct QueryResult {
    pub messages: Vec<crate::types::message::ApiMessage>,
    pub stop_reason: StopReason,
    pub tool_results: Vec<ToolResultBlock>,
}

/// Events emitted during query execution (for UI updates)
#[derive(Debug, Clone)]
pub enum QueryEvent {
    /// Streaming text from LLM
    TextDelta(String),
    /// Tool call started
    ToolCallStart { id: String, name: String },
    /// Tool call completed
    ToolCallComplete { id: String, result: ToolResultBlock },
    /// LLM response complete
    ResponseComplete { stop_reason: StopReason },
    /// Error occurred
    Error(String),
}
