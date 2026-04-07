/// Gate logic for memory auto-extraction.
///
/// Determines whether the extraction agent should run based on
/// configuration, recent activity, and throttle settings.
pub fn should_extract_memories(
    auto_memory_enabled: bool,
    main_agent_wrote_memory: bool,
    turns_since_last_extraction: usize,
    throttle_interval: usize,
) -> bool {
    if !auto_memory_enabled {
        return false;
    }
    if main_agent_wrote_memory {
        return false;
    }
    if turns_since_last_extraction < throttle_interval {
        return false;
    }
    true
}

/// Placeholder for the full extraction agent.
/// Full implementation requires forked agent infrastructure.
pub async fn execute_extract_memories(
    _memory_dir: &std::path::Path,
    _messages: &[crate::types::message::ApiMessage],
) {
    // TODO: Implement forked agent extraction
    // 1. Scan existing memory files (manifest)
    // 2. Fork agent with restricted tools
    // 3. Max 5 turns
    // 4. Advance cursor on success
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_extract_default_enabled() {
        assert!(should_extract_memories(true, false, 1, 1));
    }

    #[test]
    fn test_should_extract_disabled() {
        assert!(!should_extract_memories(false, false, 1, 1));
    }

    #[test]
    fn test_should_extract_skips_when_main_wrote() {
        assert!(!should_extract_memories(true, true, 1, 1));
    }

    #[test]
    fn test_should_extract_throttle() {
        // Below throttle interval → false
        assert!(!should_extract_memories(true, false, 2, 3));
        // At throttle interval → true
        assert!(should_extract_memories(true, false, 3, 3));
    }
}
