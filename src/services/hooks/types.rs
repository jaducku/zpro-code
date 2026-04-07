use serde::Deserialize;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// All 27 hook event names supported by the system.
pub const HOOK_EVENTS: [&str; 27] = [
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "Notification",
    "UserPromptSubmit",
    "SessionStart",
    "SessionEnd",
    "Stop",
    "StopFailure",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
    "PostCompact",
    "PermissionRequest",
    "PermissionDenied",
    "Setup",
    "TeammateIdle",
    "TaskCreated",
    "TaskCompleted",
    "Elicitation",
    "ElicitationResult",
    "ConfigChange",
    "InstructionsLoaded",
    "WorktreeCreate",
    "WorktreeRemove",
    "CwdChanged",
    "FileChanged",
];

/// A hook command configuration, tagged by type for JSON deserialization.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum HookCommand {
    Command {
        command: String,
        shell: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        once: Option<bool>,
        #[serde(rename = "async")]
        r#async: Option<bool>,
        #[serde(rename = "asyncRewake")]
        async_rewake: Option<bool>,
        #[serde(rename = "if")]
        r#if: Option<String>,
    },
    Prompt {
        prompt: String,
        model: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        once: Option<bool>,
        #[serde(rename = "if")]
        r#if: Option<String>,
    },
    Agent {
        prompt: String,
        model: Option<String>,
        timeout: Option<u64>,
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        once: Option<bool>,
        #[serde(rename = "if")]
        r#if: Option<String>,
    },
    Http {
        url: String,
        headers: Option<HashMap<String, String>>,
        #[serde(rename = "allowedEnvVars")]
        allowed_env_vars: Option<Vec<String>>,
        timeout: Option<u64>,
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        once: Option<bool>,
        #[serde(rename = "if")]
        r#if: Option<String>,
    },
}

/// A function-based hook (internal only, not serialized).
pub struct FunctionHook {
    pub id: Option<String>,
    pub timeout: Option<u64>,
    pub callback: Arc<
        dyn Fn(
                Vec<crate::types::message::ApiMessage>,
                Option<crate::utils::abort_signal::AbortSignal>,
            ) -> Pin<Box<dyn Future<Output = bool> + Send>>
            + Send
            + Sync,
    >,
    pub error_message: String,
    pub status_message: Option<String>,
}

/// A matcher entry: an optional pattern and a list of hook commands.
#[derive(Debug, Clone, Deserialize)]
pub struct HookMatcher {
    pub matcher: Option<String>,
    pub hooks: Vec<HookCommand>,
}

/// Top-level hooks settings: event name -> list of matchers.
pub type HooksSettings = HashMap<String, Vec<HookMatcher>>;

/// Output from a synchronous hook execution, parsed from JSON.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SyncHookOutput {
    #[serde(rename = "continue")]
    pub r#continue: Option<bool>,
    pub decision: Option<String>,
    pub reason: Option<String>,
    #[serde(rename = "suppressOutput")]
    pub suppress_output: Option<bool>,
    #[serde(rename = "stopReason")]
    pub stop_reason: Option<String>,
    #[serde(rename = "systemMessage")]
    pub system_message: Option<String>,
    #[serde(rename = "hookSpecificOutput")]
    pub hook_specific_output: Option<serde_json::Value>,
}

/// Result of executing a hook.
#[derive(Debug, Clone)]
pub struct HookExecutionResult {
    pub outcome: HookOutcome,
    pub additional_contexts: Vec<String>,
    pub watch_paths: Vec<String>,
    pub hook_specific: Option<serde_json::Value>,
}

/// Outcome of hook execution.
#[derive(Debug, Clone)]
pub enum HookOutcome {
    Success,
    Blocking { reason: String },
    NonBlockingError { message: String },
}

/// Runtime context for hook execution.
/// Provides LLM provider access for Prompt/Agent hooks.
pub struct HookContext {
    pub provider: Option<Arc<dyn crate::services::api::provider::LlmProvider>>,
    pub registry: Option<Arc<crate::services::api::registry::ProviderRegistry>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_events_list_has_27() {
        assert_eq!(HOOK_EVENTS.len(), 27);
    }

    #[test]
    fn test_hook_command_deserialize_command_type() {
        let json = r#"{
            "type": "command",
            "command": "echo hello",
            "shell": "bash",
            "timeout": 5000,
            "statusMessage": "Running...",
            "once": true,
            "async": false,
            "if": "Bash"
        }"#;
        let cmd: HookCommand = serde_json::from_str(json).unwrap();
        match cmd {
            HookCommand::Command {
                command,
                shell,
                timeout,
                status_message,
                once,
                r#async,
                r#if,
                ..
            } => {
                assert_eq!(command, "echo hello");
                assert_eq!(shell.as_deref(), Some("bash"));
                assert_eq!(timeout, Some(5000));
                assert_eq!(status_message.as_deref(), Some("Running..."));
                assert_eq!(once, Some(true));
                assert_eq!(r#async, Some(false));
                assert_eq!(r#if.as_deref(), Some("Bash"));
            }
            _ => panic!("Expected Command variant"),
        }
    }

    #[test]
    fn test_hook_command_deserialize_http_type() {
        let json = r#"{
            "type": "http",
            "url": "https://example.com/hook",
            "headers": {"Authorization": "Bearer token"},
            "allowedEnvVars": ["API_KEY", "SECRET"],
            "timeout": 10000
        }"#;
        let cmd: HookCommand = serde_json::from_str(json).unwrap();
        match cmd {
            HookCommand::Http {
                url,
                headers,
                allowed_env_vars,
                timeout,
                ..
            } => {
                assert_eq!(url, "https://example.com/hook");
                let h = headers.unwrap();
                assert_eq!(h.get("Authorization").unwrap(), "Bearer token");
                let vars = allowed_env_vars.unwrap();
                assert_eq!(vars, vec!["API_KEY", "SECRET"]);
                assert_eq!(timeout, Some(10000));
            }
            _ => panic!("Expected Http variant"),
        }
    }

    #[test]
    fn test_hook_command_deserialize_prompt_type() {
        let json = r#"{
            "type": "prompt",
            "prompt": "Review this code",
            "model": "claude-sonnet-4-20250514",
            "timeout": 30000
        }"#;
        let cmd: HookCommand = serde_json::from_str(json).unwrap();
        match cmd {
            HookCommand::Prompt {
                prompt,
                model,
                timeout,
                ..
            } => {
                assert_eq!(prompt, "Review this code");
                assert_eq!(model.as_deref(), Some("claude-sonnet-4-20250514"));
                assert_eq!(timeout, Some(30000));
            }
            _ => panic!("Expected Prompt variant"),
        }
    }

    #[test]
    fn test_sync_hook_output_deserialize() {
        let json = r#"{
            "continue": true,
            "decision": "approve",
            "reason": "Looks good",
            "suppressOutput": false,
            "stopReason": null,
            "systemMessage": "All clear",
            "hookSpecificOutput": {"key": "value"}
        }"#;
        let output: SyncHookOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.r#continue, Some(true));
        assert_eq!(output.decision.as_deref(), Some("approve"));
        assert_eq!(output.reason.as_deref(), Some("Looks good"));
        assert_eq!(output.suppress_output, Some(false));
        assert!(output.stop_reason.is_none());
        assert_eq!(output.system_message.as_deref(), Some("All clear"));
        assert!(output.hook_specific_output.is_some());
    }
}
