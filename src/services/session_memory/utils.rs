use std::path::{Path, PathBuf};

pub const EXTRACTION_WAIT_TIMEOUT_MS: u64 = 15_000;
pub const EXTRACTION_STALE_THRESHOLD_MS: u64 = 60_000;

pub const DEFAULT_SESSION_MEMORY_TEMPLATE: &str = r#"
# Session Title
_A short and distinctive 5-10 word descriptive title for the session. Super info dense, no filler_

# Current State
_What is actively being worked on right now? Pending tasks not yet completed. Immediate next steps._

# Task specification
_What did the user ask to build? Any design decisions or other explanatory context_

# Files and Functions
_What are the important files? In short, what do they contain and why are they relevant?_

# Workflow
_What bash commands are usually run and in what order? How to interpret their output if not obvious?_

# Errors & Corrections
_Errors encountered and how they were fixed. What did the user correct? What approaches failed and should not be tried again?_

# Codebase and System Documentation
_What are the important system components? How do they work/fit together?_

# Learnings
_What has worked well? What has not? What to avoid? Do not duplicate items from other sections_

# Key results
_If the user asked a specific output such as an answer to a question, a table, or other document, repeat the exact result here_

# Worklog
_Step by step, what was attempted, done? Very terse summary for each step_
"#;

/// Get the session memory directory path: ~/.claude/session-memory/
pub fn get_session_memory_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".claude").join("session-memory")
}

/// Get the session memory file path for a specific session.
pub fn get_session_memory_path(session_id: &str) -> PathBuf {
    get_session_memory_dir().join(format!("{session_id}.md"))
}

/// Read session memory content from disk. Returns None if file doesn't exist.
pub async fn get_session_memory_content(path: &Path) -> Option<String> {
    tokio::fs::read_to_string(path).await.ok()
}

/// Check if session memory content is empty (matches template).
pub fn is_session_memory_empty(content: &str, template: &str) -> bool {
    content.trim() == template.trim()
}

/// Wait for in-progress extraction to complete (15s timeout, 1s poll, 60s stale skip).
pub async fn wait_for_extraction(state: &super::types::SessionMemoryState) {
    let start = std::time::Instant::now();
    loop {
        let started_at = state.extraction_started_at.read().await;
        match *started_at {
            None => return,
            Some(at) => {
                if at.elapsed().as_millis() > EXTRACTION_STALE_THRESHOLD_MS as u128 {
                    return;
                }
            }
        }
        drop(started_at);
        if start.elapsed().as_millis() > EXTRACTION_WAIT_TIMEOUT_MS as u128 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_memory_path() {
        let path = get_session_memory_path("test-session-123");
        assert!(path.to_string_lossy().contains("session-memory"));
        assert!(path.to_string_lossy().ends_with("test-session-123.md"));
    }

    #[test]
    fn test_is_session_memory_empty_true() {
        let template = DEFAULT_SESSION_MEMORY_TEMPLATE;
        assert!(is_session_memory_empty(template, template));
    }

    #[test]
    fn test_is_session_memory_empty_false() {
        let template = DEFAULT_SESSION_MEMORY_TEMPLATE;
        let content = format!("{template}\n\nSome actual content");
        assert!(!is_session_memory_empty(&content, template));
    }
}
