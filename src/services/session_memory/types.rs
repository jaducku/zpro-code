use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};

pub struct SessionMemoryConfig {
    pub minimum_message_tokens_to_init: u64,
    pub minimum_tokens_between_update: u64,
    pub tool_calls_between_updates: usize,
}

impl Default for SessionMemoryConfig {
    fn default() -> Self {
        Self {
            minimum_message_tokens_to_init: 10_000,
            minimum_tokens_between_update: 5_000,
            tool_calls_between_updates: 3,
        }
    }
}

pub struct SessionMemoryCompactConfig {
    pub min_tokens: u64,
    pub min_text_block_messages: usize,
    pub max_tokens: u64,
}

impl Default for SessionMemoryCompactConfig {
    fn default() -> Self {
        Self {
            min_tokens: 10_000,
            min_text_block_messages: 5,
            max_tokens: 40_000,
        }
    }
}

pub struct SessionMemoryState {
    pub config: SessionMemoryConfig,
    pub compact_config: SessionMemoryCompactConfig,
    pub session_id: String,
    pub initialized: AtomicBool,
    /// Index of the last message summarized into session memory.
    /// Index-based since ApiMessage has no id field.
    pub last_summarized_index: RwLock<Option<usize>>,
    pub extraction_started_at: RwLock<Option<Instant>>,
    pub tokens_at_last_extraction: AtomicU64,
    pub last_memory_message_index: RwLock<Option<usize>>,
    pub extraction_lock: Mutex<()>,
}

impl SessionMemoryState {
    pub fn new(session_id: String) -> Self {
        Self {
            config: SessionMemoryConfig::default(),
            compact_config: SessionMemoryCompactConfig::default(),
            session_id,
            initialized: AtomicBool::new(false),
            last_summarized_index: RwLock::new(None),
            extraction_started_at: RwLock::new(None),
            tokens_at_last_extraction: AtomicU64::new(0),
            last_memory_message_index: RwLock::new(None),
            extraction_lock: Mutex::new(()),
        }
    }

    pub async fn reset_last_summarized_index(&self) {
        *self.last_summarized_index.write().await = None;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    #[test]
    fn test_default_config() {
        let config = SessionMemoryConfig::default();
        assert_eq!(config.minimum_message_tokens_to_init, 10_000);
        assert_eq!(config.minimum_tokens_between_update, 5_000);
        assert_eq!(config.tool_calls_between_updates, 3);
    }

    #[test]
    fn test_default_compact_config() {
        let config = SessionMemoryCompactConfig::default();
        assert_eq!(config.min_tokens, 10_000);
        assert_eq!(config.min_text_block_messages, 5);
        assert_eq!(config.max_tokens, 40_000);
    }

    #[test]
    fn test_new_state() {
        let state = SessionMemoryState::new("test-123".into());
        assert_eq!(state.session_id, "test-123");
        assert!(!state.initialized.load(Ordering::SeqCst));
    }
}
