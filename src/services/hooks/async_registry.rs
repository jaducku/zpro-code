use std::collections::HashMap;

use tokio::process::Child;

use crate::services::hooks::types::SyncHookOutput;

/// A background hook process that hasn't completed yet.
pub struct PendingAsyncHook {
    pub hook_id: String,
    pub child: Child,
    pub command: String,
    pub async_rewake: bool,
}

/// The result of a completed async hook.
pub struct AsyncHookResponse {
    pub hook_id: String,
    pub output: SyncHookOutput,
    pub exit_code: i32,
}

/// Registry that tracks background hook processes.
pub struct AsyncHookRegistry {
    pending: HashMap<String, PendingAsyncHook>,
}

impl AsyncHookRegistry {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    pub fn register_pending(&mut self, hook: PendingAsyncHook) {
        self.pending.insert(hook.hook_id.clone(), hook);
    }

    /// Check all pending hooks for completion via `try_wait()`.
    /// Returns completed hooks with their parsed output.
    pub fn check_for_responses(&mut self) -> Vec<AsyncHookResponse> {
        let mut completed_ids = Vec::new();
        let mut responses = Vec::new();

        for (id, hook) in self.pending.iter_mut() {
            match hook.child.try_wait() {
                Ok(Some(status)) => {
                    let exit_code = status.code().unwrap_or(-1);

                    // Read stdout from the child if available
                    let stdout_str = String::new();
                    // Note: In practice, stdout would be captured via piped output
                    // and read here. For now we parse whatever we have.
                    let output = parse_async_stdout(&stdout_str);

                    completed_ids.push(id.clone());
                    responses.push(AsyncHookResponse {
                        hook_id: id.clone(),
                        output,
                        exit_code,
                    });
                }
                Ok(None) => {
                    // Still running
                }
                Err(_) => {
                    // Error checking status — treat as completed with error
                    completed_ids.push(id.clone());
                    responses.push(AsyncHookResponse {
                        hook_id: id.clone(),
                        output: SyncHookOutput::default(),
                        exit_code: -1,
                    });
                }
            }
        }

        for id in completed_ids {
            self.pending.remove(&id);
        }

        responses
    }

    /// Kill all remaining pending hooks on shutdown.
    pub fn finalize_pending(&mut self) {
        for (_, mut hook) in self.pending.drain() {
            // Best-effort kill; ignore errors.
            let _ = hook.child.start_kill();
        }
    }
}

/// Parse stdout from an async hook process into a `SyncHookOutput`.
///
/// - Splits stdout by newlines
/// - Finds the first line starting with `{`
/// - Tries to parse as JSON
/// - If parsed JSON contains an "async" key, skips it (initial async ack)
/// - First valid non-ack JSON becomes the response
/// - If no JSON found, returns `SyncHookOutput::default()`
pub fn parse_async_stdout(stdout: &str) -> SyncHookOutput {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('{') {
            continue;
        }

        // Try to parse as generic JSON first to check for "async" key
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if value.get("async").is_some() {
                // This is the async acknowledgment — skip it
                continue;
            }

            // Try to deserialize as SyncHookOutput
            if let Ok(output) = serde_json::from_value::<SyncHookOutput>(value) {
                return output;
            }
        }
    }

    SyncHookOutput::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_empty() {
        let mut registry = AsyncHookRegistry::new();
        let responses = registry.check_for_responses();
        assert!(responses.is_empty());
    }

    #[test]
    fn test_parse_async_response_first_json_line() {
        // JSON is on the second line (first line is plain text)
        let stdout = "Starting hook...\n{\"decision\":\"approve\",\"reason\":\"all good\"}";
        let output = parse_async_stdout(stdout);
        assert_eq!(output.decision.as_deref(), Some("approve"));
        assert_eq!(output.reason.as_deref(), Some("all good"));
    }

    #[test]
    fn test_parse_async_response_skips_async_key() {
        // First JSON line is the async ack, second is the real response
        let stdout =
            "{\"async\":true}\n{\"decision\":\"deny\",\"reason\":\"not allowed\"}";
        let output = parse_async_stdout(stdout);
        assert_eq!(output.decision.as_deref(), Some("deny"));
        assert_eq!(output.reason.as_deref(), Some("not allowed"));
    }

    #[test]
    fn test_parse_async_response_no_json() {
        let stdout = "just some plain text\nno json here";
        let output = parse_async_stdout(stdout);
        assert!(output.decision.is_none());
        assert!(output.reason.is_none());
        assert!(output.r#continue.is_none());
    }
}
