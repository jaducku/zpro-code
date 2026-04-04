use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;

use crate::commands::registry::CommandRegistry;
use crate::services::api::provider::LlmProvider;
use crate::services::api::types::SystemBlock;
use crate::services::hooks::HooksSettings;
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
    pub question_tx: mpsc::UnboundedSender<UserQuestionAsk>,
    pub hooks: HooksSettings,
    pub cwd: PathBuf,
    /// Maximum agentic turns (tool-use loops). None = unlimited (default 50 in engine).
    pub max_turns: Option<u32>,
    /// Maximum budget in USD. None = unlimited.
    pub max_budget_usd: Option<f64>,
    pub commands: Arc<CommandRegistry>,
    pub provider: Arc<dyn LlmProvider>,
    pub app_state: Arc<tokio::sync::RwLock<crate::state::AppState>>,
    pub mcp_manager: Option<Arc<crate::services::mcp::connection_manager::McpConnectionManager>>,
}

/// Sent to TUI when a tool wants to ask the user a question
pub struct UserQuestionAsk {
    pub tool_use_id: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub reply_tx: tokio::sync::oneshot::Sender<UserQuestionReply>,
}

pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

pub enum UserQuestionReply {
    Answer(String),
    Dismissed,
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
    /// Token usage update
    Usage { input_tokens: u64, output_tokens: u64 },
    /// Error occurred
    Error(String),
    /// Brief/SendUserMessage output for TUI rendering
    BriefMessage {
        message: String,
        attachments: Vec<BriefAttachment>,
        status: BriefStatus,
    },
}

#[derive(Debug, Clone)]
pub struct BriefAttachment {
    pub path: std::path::PathBuf,
    pub size: u64,
    pub is_image: bool,
}

#[derive(Debug, Clone)]
pub enum BriefStatus {
    Normal,
    Proactive,
}
