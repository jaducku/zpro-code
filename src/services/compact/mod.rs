/// Check if compaction is needed (80% threshold).
pub fn needs_compaction(current_tokens: u64, context_window: u64) -> bool {
    let threshold = context_window * 80 / 100;
    current_tokens > threshold
}

/// Build a compaction prompt for the LLM.
pub fn build_compact_prompt(custom_instructions: Option<&str>) -> String {
    let base = "Summarize the conversation so far in a concise way that preserves \
                all important context, decisions made, files modified, and current goals.";
    match custom_instructions {
        Some(inst) => format!("{}\n\nAdditional instructions: {}", base, inst),
        None => base.to_string(),
    }
}

pub struct CompactResult {
    pub summary: String,
    pub messages_removed: usize,
    pub tokens_before: u64,
    pub tokens_after: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_compaction_under() {
        assert!(!needs_compaction(1000, 128_000));
    }

    #[test]
    fn test_needs_compaction_over() {
        assert!(needs_compaction(110_000, 128_000));
    }

    #[test]
    fn test_build_compact_prompt_default() {
        let p = build_compact_prompt(None);
        assert!(p.contains("Summarize"));
    }

    #[test]
    fn test_build_compact_prompt_custom() {
        let p = build_compact_prompt(Some("focus on code changes"));
        assert!(p.contains("focus on code changes"));
    }
}
