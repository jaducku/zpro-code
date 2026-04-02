use serde::{Deserialize, Serialize};

use crate::tool::ToolDefinition;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, StopReason};

/// Request to create a message (sent to LLM provider)
#[derive(Debug, Clone)]
pub struct CreateMessageRequest {
    pub model: String,
    pub messages: Vec<ApiMessage>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub max_tokens: u32,
    pub system: Option<Vec<SystemBlock>>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

/// System prompt block
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SystemBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

/// Provider capabilities
#[derive(Debug, Clone, Default)]
pub struct Capabilities {
    pub tool_use: bool,
    pub vision: bool,
    pub streaming: bool,
    pub extended_thinking: bool,
}

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_window: u64,
    pub max_output_tokens: u32,
    pub capabilities: Capabilities,
}

/// Streaming event from LLM
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Message creation started
    MessageStart { id: String },
    /// New content block started
    ContentBlockStart {
        index: usize,
        content_block: ContentBlock,
    },
    /// Delta for current content block
    ContentBlockDelta { index: usize, delta: ContentDelta },
    /// Content block completed
    ContentBlockStop { index: usize },
    /// Message completed
    MessageStop { stop_reason: StopReason },
    /// Usage info
    Usage(UsageInfo),
}

/// Content delta in a streaming response
#[derive(Debug, Clone)]
pub enum ContentDelta {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
    ThinkingDelta { thinking: String },
}

/// Token usage information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_block_serde() {
        let block = SystemBlock::Text {
            text: "You are helpful".into(),
        };
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "You are helpful");
    }

    #[test]
    fn test_usage_info_default() {
        let usage = UsageInfo::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert!(usage.cache_creation_input_tokens.is_none());
    }

    #[test]
    fn test_capabilities_default() {
        let caps = Capabilities::default();
        assert!(!caps.tool_use);
        assert!(!caps.vision);
    }
}
