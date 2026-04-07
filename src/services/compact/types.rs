use crate::types::message::ApiMessage;

/// Result of any compaction operation.
pub struct CompactResult {
    pub messages: Vec<ApiMessage>,
    pub tokens_freed: u64,
    pub stage: CompactStage,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompactStage {
    Snip,
    Microcompact,
    FullCompact,
    Reactive,
}

impl std::fmt::Display for CompactStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snip => write!(f, "snip"),
            Self::Microcompact => write!(f, "microcompact"),
            Self::FullCompact => write!(f, "full"),
            Self::Reactive => write!(f, "reactive"),
        }
    }
}

pub const COMPACT_BOUNDARY_PREFIX: &str = "[Context compacted";

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
    fn test_boundary_prefix() {
        assert!(COMPACT_BOUNDARY_PREFIX.starts_with("[Context"));
    }
}
