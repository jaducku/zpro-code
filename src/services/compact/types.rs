use crate::types::message::ApiMessage;

/// Result of any compaction operation.
#[derive(Debug)]
pub struct CompactResult {
    pub messages: Vec<ApiMessage>,
    pub tokens_freed: u64,
    pub stage: CompactStage,
    pub pre_compact_token_count: Option<u64>,
    pub post_compact_token_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompactStage {
    Snip,
    Microcompact,
    FullCompact,
    Reactive,
    SessionMemory,
}

impl std::fmt::Display for CompactStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snip => write!(f, "snip"),
            Self::Microcompact => write!(f, "microcompact"),
            Self::FullCompact => write!(f, "full"),
            Self::Reactive => write!(f, "reactive"),
            Self::SessionMemory => write!(f, "session_memory"),
        }
    }
}

pub const COMPACT_BOUNDARY_PREFIX: &str = "[Context compacted";

pub const ERROR_MESSAGE_USER_ABORT: &str = "Compaction canceled.";
pub const ERROR_MESSAGE_NOT_ENOUGH_MESSAGES: &str = "No messages to compact";
pub const ERROR_MESSAGE_INCOMPLETE_RESPONSE: &str = "Compaction produced incomplete response";

pub const AUTOCOMPACT_BUFFER_TOKENS: u64 = 13_000;
pub const COMPACT_MAX_OUTPUT_TOKENS: u32 = 20_000;
pub const MAX_PTL_RETRIES: u32 = 3;
pub const MAX_CONSECUTIVE_COMPACT_FAILURES: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_stage_display() {
        assert_eq!(CompactStage::Snip.to_string(), "snip");
        assert_eq!(CompactStage::Microcompact.to_string(), "microcompact");
        assert_eq!(CompactStage::FullCompact.to_string(), "full");
        assert_eq!(CompactStage::Reactive.to_string(), "reactive");
    }

    #[test]
    fn test_compact_stage_session_memory_display() {
        assert_eq!(CompactStage::SessionMemory.to_string(), "session_memory");
    }

    #[test]
    fn test_error_constants() {
        assert!(!ERROR_MESSAGE_USER_ABORT.is_empty());
        assert!(!ERROR_MESSAGE_NOT_ENOUGH_MESSAGES.is_empty());
        assert!(!ERROR_MESSAGE_INCOMPLETE_RESPONSE.is_empty());
    }

    #[test]
    fn test_boundary_prefix() {
        assert!(COMPACT_BOUNDARY_PREFIX.starts_with("[Context"));
    }
}
