use crate::services::api::types::SystemBlock;
use crate::tool::{Tool, ToolResultBlock};
use crate::types::message::StopReason;

/// Context for a query execution
pub struct QueryContext {
    pub messages: Vec<crate::types::message::ApiMessage>,
    pub tools: Vec<Box<dyn Tool>>,
    pub system_prompt: Option<Vec<SystemBlock>>,
    pub model: String,
    pub max_tokens: u32,
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
