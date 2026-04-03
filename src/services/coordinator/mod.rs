//! Coordinator mode detection and tool filtering.
//!
//! When coordinator mode is active, the main session acts as an orchestrator
//! that delegates work to async worker agents. This module provides the
//! constants, detection, and tool-filtering logic for both sides.

pub mod worker;

use std::collections::HashMap;
use std::path::Path;

/// Tools the coordinator (orchestrator) is allowed to use.
pub const COORDINATOR_ALLOWED_TOOLS: &[&str] =
    &["Agent", "TaskStop", "SendMessage", "SyntheticOutput"];

/// Tools an async worker agent is allowed to use.
pub const ASYNC_AGENT_ALLOWED_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Glob",
    "Grep",
    "Bash",
    "WebSearch",
    "WebFetch",
    "TodoWrite",
    "NotebookEdit",
    "Skill",
    "ToolSearch",
    "SyntheticOutput",
    "EnterWorktree",
    "ExitWorktree",
];

/// Returns `true` when the `ATOM_CODE_COORDINATOR_MODE` env var is truthy.
/// Reads the env var live on every call (no caching).
pub fn is_coordinator_mode() -> bool {
    matches!(
        std::env::var("ATOM_CODE_COORDINATOR_MODE")
            .unwrap_or_default()
            .as_str(),
        "1" | "true" | "yes"
    )
}

/// Keep only tools that appear in [`COORDINATOR_ALLOWED_TOOLS`].
pub fn filter_coordinator_tools<'a>(tool_names: &[&'a str]) -> Vec<&'a str> {
    tool_names
        .iter()
        .copied()
        .filter(|name| COORDINATOR_ALLOWED_TOOLS.contains(name))
        .collect()
}

/// Keep only tools that appear in [`ASYNC_AGENT_ALLOWED_TOOLS`].
pub fn filter_worker_tools<'a>(tool_names: &[&'a str]) -> Vec<&'a str> {
    tool_names
        .iter()
        .copied()
        .filter(|name| ASYNC_AGENT_ALLOWED_TOOLS.contains(name))
        .collect()
}

/// Compare a stored session mode against the current env-var state.
///
/// If they mismatch, flips `ATOM_CODE_COORDINATOR_MODE` so that
/// [`is_coordinator_mode`] returns the correct value for the resumed session.
/// Returns a warning message when the mode was switched, or `None` if no
/// switch was needed.
pub fn match_session_mode(stored_mode: Option<&str>) -> Option<String> {
    let stored = stored_mode?;

    let current_is_coordinator = is_coordinator_mode();
    let session_is_coordinator = stored == "coordinator";

    if current_is_coordinator == session_is_coordinator {
        return None;
    }

    // Flip the env var -- is_coordinator_mode() reads it live, no caching.
    if session_is_coordinator {
        std::env::set_var("ATOM_CODE_COORDINATOR_MODE", "1");
    } else {
        std::env::remove_var("ATOM_CODE_COORDINATOR_MODE");
    }

    if session_is_coordinator {
        Some("Entered coordinator mode to match resumed session.".to_string())
    } else {
        Some("Exited coordinator mode to match resumed session.".to_string())
    }
}

/// Internal tools that workers use but should not appear in the
/// user-facing "worker tools" context string.
const INTERNAL_WORKER_TOOLS: &[&str] = &["SyntheticOutput"];

/// Build context strings describing worker capabilities for the coordinator.
///
/// Returns a map with a `workerToolsContext` key when coordinator mode is
/// active, or an empty map otherwise.
pub fn get_coordinator_user_context(
    mcp_server_names: &[String],
    scratchpad: Option<&Path>,
) -> HashMap<String, String> {
    if !is_coordinator_mode() {
        return HashMap::new();
    }

    let mut worker_tools: Vec<&str> = ASYNC_AGENT_ALLOWED_TOOLS
        .iter()
        .copied()
        .filter(|t| !INTERNAL_WORKER_TOOLS.contains(t))
        .collect();
    worker_tools.sort();

    let tools_csv = worker_tools.join(", ");
    let mut content =
        format!("Workers spawned via the Agent tool have access to these tools: {tools_csv}");

    if !mcp_server_names.is_empty() {
        let names = mcp_server_names.join(", ");
        content.push_str(&format!(
            "\n\nWorkers also have access to MCP tools from connected MCP servers: {names}"
        ));
    }

    if let Some(dir) = scratchpad {
        content.push_str(&format!(
            "\n\nScratchpad directory: {}\nWorkers can read and write here without permission prompts. Use this for durable cross-worker knowledge \u{2014} structure files however fits the work.",
            dir.display()
        ));
    }

    let mut map = HashMap::new();
    map.insert("workerToolsContext".to_string(), content);
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_coordinator_mode_default_false() {
        // Ensure the env var is unset for this test.
        std::env::remove_var("ATOM_CODE_COORDINATOR_MODE");
        assert!(!is_coordinator_mode());
    }

    #[test]
    fn test_coordinator_allowed_tools() {
        assert_eq!(COORDINATOR_ALLOWED_TOOLS.len(), 4);
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"Agent"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"TaskStop"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"SendMessage"));
        assert!(COORDINATOR_ALLOWED_TOOLS.contains(&"SyntheticOutput"));
    }

    #[test]
    fn test_async_agent_allowed_tools() {
        assert!(ASYNC_AGENT_ALLOWED_TOOLS.contains(&"Read"));
        assert!(ASYNC_AGENT_ALLOWED_TOOLS.contains(&"Write"));
        assert!(ASYNC_AGENT_ALLOWED_TOOLS.contains(&"Bash"));
        // Agent must NOT be in the worker list (prevents recursion).
        assert!(!ASYNC_AGENT_ALLOWED_TOOLS.contains(&"Agent"));
    }

    #[test]
    fn test_filter_tools_for_coordinator() {
        let all = &[
            "Agent",
            "Bash",
            "Read",
            "SendMessage",
            "Grep",
            "TaskStop",
            "SyntheticOutput",
            "Write",
        ];
        let filtered = filter_coordinator_tools(all);
        assert_eq!(
            filtered,
            vec!["Agent", "SendMessage", "TaskStop", "SyntheticOutput"]
        );
    }
}
