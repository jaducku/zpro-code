use std::collections::HashMap;
use std::path::Path;

use crate::services::hooks::command_hook::{execute_command_hook, CommandHookResult, HookEnv};
use crate::services::hooks::matcher::{matches_pattern, parse_if_condition};
use crate::services::hooks::types::{
    HookCommand, HookExecutionResult, HookOutcome, HooksSettings, SyncHookOutput,
};
#[cfg(test)]
use crate::services::hooks::types::HookMatcher;

/// Events that are always emitted (used for documentation/future filtering at a higher level).
const ALWAYS_EMITTED: &[&str] = &["SessionStart", "Setup"];

/// Build base environment variables for hook execution.
fn build_base_env(event: &str, cwd: &Path) -> HookEnv {
    let mut env = HashMap::new();
    env.insert("ATOM_HOOK_TYPE".to_string(), event.to_string());
    env.insert(
        "ATOM_CWD".to_string(),
        cwd.to_string_lossy().to_string(),
    );
    env.insert("ATOM_SESSION_ID".to_string(), String::new());
    env.insert("ATOM_TRANSCRIPT_PATH".to_string(), String::new());
    env.insert("ATOM_PERMISSION_MODE".to_string(), String::new());
    env.insert("ATOM_AGENT_ID".to_string(), String::new());
    env.insert("ATOM_AGENT_TYPE".to_string(), String::new());
    env
}

/// Extract the `if` field from any HookCommand variant.
fn get_if_condition(hook: &HookCommand) -> Option<String> {
    match hook {
        HookCommand::Command { r#if, .. } => r#if.clone(),
        HookCommand::Prompt { r#if, .. } => r#if.clone(),
        HookCommand::Agent { r#if, .. } => r#if.clone(),
        HookCommand::Http { r#if, .. } => r#if.clone(),
    }
}

/// Parse hook stdout to extract JSON output.
/// Finds the first line starting with `{` and tries to parse it as SyncHookOutput.
fn parse_hook_output(stdout: &str) -> SyncHookOutput {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(output) = serde_json::from_str::<SyncHookOutput>(trimmed) {
                return output;
            }
        }
    }
    // Default: no special output
    SyncHookOutput {
        r#continue: None,
        decision: None,
        reason: None,
        suppress_output: None,
        stop_reason: None,
        system_message: None,
        hook_specific_output: None,
    }
}

/// Execute a single hook command and return the execution result.
async fn execute_single_hook(
    hook: &HookCommand,
    env: &HookEnv,
    cwd: &Path,
) -> HookExecutionResult {
    match hook {
        HookCommand::Command {
            command,
            shell,
            timeout,
            ..
        } => {
            let result = execute_command_hook(
                command,
                shell.as_deref(),
                env,
                cwd,
                *timeout,
            )
            .await;

            match result {
                CommandHookResult::Success { output } => {
                    let parsed = parse_hook_output(&output.stdout);
                    if parsed.decision.as_deref() == Some("block") {
                        let reason = parsed
                            .reason
                            .unwrap_or_else(|| "Blocked by hook".to_string());
                        HookExecutionResult {
                            outcome: HookOutcome::Blocking { reason },
                            additional_contexts: Vec::new(),
                            watch_paths: Vec::new(),
                            hook_specific: parsed.hook_specific_output,
                        }
                    } else {
                        HookExecutionResult {
                            outcome: HookOutcome::Success,
                            additional_contexts: Vec::new(),
                            watch_paths: Vec::new(),
                            hook_specific: parsed.hook_specific_output,
                        }
                    }
                }
                CommandHookResult::Blocking { reason, output } => {
                    let parsed = parse_hook_output(&output.stdout);
                    let final_reason = parsed
                        .reason
                        .or(Some(reason))
                        .unwrap_or_else(|| "Blocked by hook".to_string());
                    HookExecutionResult {
                        outcome: HookOutcome::Blocking {
                            reason: final_reason,
                        },
                        additional_contexts: Vec::new(),
                        watch_paths: Vec::new(),
                        hook_specific: parsed.hook_specific_output,
                    }
                }
                CommandHookResult::NonBlockingError { message } => HookExecutionResult {
                    outcome: HookOutcome::NonBlockingError { message },
                    additional_contexts: Vec::new(),
                    watch_paths: Vec::new(),
                    hook_specific: None,
                },
            }
        }
        HookCommand::Http {
            url,
            headers,
            allowed_env_vars,
            timeout,
            ..
        } => {
            let input_json = serde_json::to_string(&serde_json::json!({
                "hook_event": env.get("ATOM_HOOK_TYPE").unwrap_or(&String::new()),
                "tool_name": env.get("ATOM_TOOL_NAME").unwrap_or(&String::new()),
                "tool_input": env.get("ATOM_TOOL_INPUT").unwrap_or(&String::new()),
            }))
            .unwrap_or_default();

            let hdrs = headers.as_ref().cloned().unwrap_or_default();
            let allowed = allowed_env_vars.as_ref().cloned().unwrap_or_default();

            match crate::services::hooks::http_hook::execute_http_hook(
                url, &hdrs, &allowed, env, &input_json, *timeout,
            )
            .await
            {
                Ok(result) => {
                    if result.status >= 200 && result.status < 300 {
                        let parsed = parse_hook_output(&result.body);
                        HookExecutionResult {
                            outcome: if parsed.decision.as_deref() == Some("block") {
                                HookOutcome::Blocking {
                                    reason: parsed
                                        .reason
                                        .unwrap_or_else(|| "Blocked by HTTP hook".to_string()),
                                }
                            } else {
                                HookOutcome::Success
                            },
                            additional_contexts: Vec::new(),
                            watch_paths: Vec::new(),
                            hook_specific: parsed.hook_specific_output,
                        }
                    } else {
                        HookExecutionResult {
                            outcome: HookOutcome::Blocking {
                                reason: format!("HTTP hook returned status {}", result.status),
                            },
                            additional_contexts: Vec::new(),
                            watch_paths: Vec::new(),
                            hook_specific: None,
                        }
                    }
                }
                Err(e) => HookExecutionResult {
                    outcome: HookOutcome::NonBlockingError { message: e },
                    additional_contexts: Vec::new(),
                    watch_paths: Vec::new(),
                    hook_specific: None,
                },
            }
        }
        HookCommand::Prompt { .. } => HookExecutionResult {
            outcome: HookOutcome::NonBlockingError {
                message: "Prompt hooks require LLM provider (not yet wired)".to_string(),
            },
            additional_contexts: Vec::new(),
            watch_paths: Vec::new(),
            hook_specific: None,
        },
        HookCommand::Agent { .. } => HookExecutionResult {
            outcome: HookOutcome::NonBlockingError {
                message: "Agent hooks require multi-turn agent infrastructure (not yet implemented)"
                    .to_string(),
            },
            additional_contexts: Vec::new(),
            watch_paths: Vec::new(),
            hook_specific: None,
        },
    }
}

/// Check whether a hook's `if` condition passes for the given match_query.
///
/// If the hook has no `if` condition, it passes.
/// If it has one, parse it: `"ToolName"` must match match_query,
/// and `"ToolName(pattern)"` must also match the pattern against match_query.
fn check_if_condition(hook: &HookCommand, match_query: &str) -> bool {
    let condition = match get_if_condition(hook) {
        None => return true,
        Some(c) => c,
    };

    match parse_if_condition(&condition) {
        None => false, // malformed condition
        Some((tool_name, pattern)) => {
            if tool_name != match_query {
                return false;
            }
            if let Some(pat) = pattern {
                matches_pattern(&Some(pat), match_query)
            } else {
                true
            }
        }
    }
}

/// Execute all hooks matching the given event and query in parallel.
///
/// Returns an aggregated result: if ANY hook blocks, the whole result is Blocking.
pub async fn execute_event_hooks(
    event: &str,
    match_query: &str,
    input: &serde_json::Value,
    settings: &HooksSettings,
    cwd: &Path,
) -> HookExecutionResult {
    let success_result = HookExecutionResult {
        outcome: HookOutcome::Success,
        additional_contexts: Vec::new(),
        watch_paths: Vec::new(),
        hook_specific: None,
    };

    // 1. Look up matchers for the given event
    let matchers = match settings.get(event) {
        Some(m) => m,
        None => return success_result,
    };

    // 2-4. Collect all matching hooks
    let mut matching_hooks: Vec<&HookCommand> = Vec::new();
    for matcher in matchers {
        if !matches_pattern(&matcher.matcher, match_query) {
            continue;
        }
        for hook in &matcher.hooks {
            if check_if_condition(hook, match_query) {
                matching_hooks.push(hook);
            }
        }
    }

    // Filter out HTTP hooks for SessionStart/Setup events (deadlock prevention)
    if ALWAYS_EMITTED.contains(&event) {
        matching_hooks.retain(|hook| !matches!(hook, HookCommand::Http { .. }));
    }

    if matching_hooks.is_empty() {
        return success_result;
    }

    // 5-6. Build environment
    let mut env = build_base_env(event, cwd);
    env.insert("ATOM_TOOL_NAME".to_string(), match_query.to_string());
    env.insert(
        "ATOM_TOOL_INPUT".to_string(),
        serde_json::to_string(input).unwrap_or_default(),
    );

    // 7. Execute all matching hooks in parallel
    let futures: Vec<_> = matching_hooks
        .iter()
        .map(|hook| execute_single_hook(hook, &env, cwd))
        .collect();

    let results = futures::future::join_all(futures).await;

    // 8. Aggregate results
    let mut combined = HookExecutionResult {
        outcome: HookOutcome::Success,
        additional_contexts: Vec::new(),
        watch_paths: Vec::new(),
        hook_specific: None,
    };

    for result in results {
        // Accumulate contexts and watch paths
        combined.additional_contexts.extend(result.additional_contexts);
        combined.watch_paths.extend(result.watch_paths);

        // Use last non-None hook_specific
        if result.hook_specific.is_some() {
            combined.hook_specific = result.hook_specific;
        }

        // If any hook blocks, use the first blocking reason
        if let HookOutcome::Blocking { reason } = result.outcome {
            if matches!(combined.outcome, HookOutcome::Success) {
                combined.outcome = HookOutcome::Blocking { reason };
            }
        }
    }

    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn empty_settings() -> HooksSettings {
        HashMap::new()
    }

    fn settings_with_hook(event: &str, matcher: Option<&str>, hook: HookCommand) -> HooksSettings {
        let mut settings = HashMap::new();
        settings.insert(
            event.to_string(),
            vec![HookMatcher {
                matcher: matcher.map(|s| s.to_string()),
                hooks: vec![hook],
            }],
        );
        settings
    }

    #[tokio::test]
    async fn test_execute_hooks_no_matching() {
        let dir = TempDir::new().unwrap();
        let result = execute_event_hooks(
            "PreToolUse",
            "Bash",
            &serde_json::json!({}),
            &empty_settings(),
            dir.path(),
        )
        .await;
        assert!(matches!(result.outcome, HookOutcome::Success));
    }

    #[tokio::test]
    async fn test_execute_hooks_matching_allows() {
        let dir = TempDir::new().unwrap();
        let settings = settings_with_hook(
            "PreToolUse",
            None, // matches everything
            HookCommand::Command {
                command: "true".to_string(),
                shell: None,
                timeout: None,
                status_message: None,
                once: None,
                r#async: None,
                async_rewake: None,
                r#if: None,
            },
        );
        let result = execute_event_hooks(
            "PreToolUse",
            "Bash",
            &serde_json::json!({}),
            &settings,
            dir.path(),
        )
        .await;
        assert!(matches!(result.outcome, HookOutcome::Success));
    }

    #[tokio::test]
    async fn test_execute_hooks_blocking_result() {
        let dir = TempDir::new().unwrap();
        let settings = settings_with_hook(
            "PreToolUse",
            None,
            HookCommand::Command {
                command: "exit 2".to_string(),
                shell: None,
                timeout: None,
                status_message: None,
                once: None,
                r#async: None,
                async_rewake: None,
                r#if: None,
            },
        );
        let result = execute_event_hooks(
            "PreToolUse",
            "Bash",
            &serde_json::json!({}),
            &settings,
            dir.path(),
        )
        .await;
        assert!(
            matches!(result.outcome, HookOutcome::Blocking { .. }),
            "Expected Blocking, got {:?}",
            result.outcome
        );
    }

    #[tokio::test]
    async fn test_execute_hooks_matcher_filters() {
        let dir = TempDir::new().unwrap();
        let settings = settings_with_hook(
            "PreToolUse",
            Some("Edit"), // only matches "Edit"
            HookCommand::Command {
                command: "exit 2".to_string(),
                shell: None,
                timeout: None,
                status_message: None,
                once: None,
                r#async: None,
                async_rewake: None,
                r#if: None,
            },
        );
        // Query is "Bash" which doesn't match "Edit"
        let result = execute_event_hooks(
            "PreToolUse",
            "Bash",
            &serde_json::json!({}),
            &settings,
            dir.path(),
        )
        .await;
        assert!(
            matches!(result.outcome, HookOutcome::Success),
            "Expected Success (filtered out), got {:?}",
            result.outcome
        );
    }

    #[tokio::test]
    async fn test_execute_hooks_json_block() {
        let dir = TempDir::new().unwrap();
        let settings = settings_with_hook(
            "PreToolUse",
            None,
            HookCommand::Command {
                command: r#"echo '{"decision":"block","reason":"test"}'"#.to_string(),
                shell: None,
                timeout: None,
                status_message: None,
                once: None,
                r#async: None,
                async_rewake: None,
                r#if: None,
            },
        );
        let result = execute_event_hooks(
            "PreToolUse",
            "Bash",
            &serde_json::json!({}),
            &settings,
            dir.path(),
        )
        .await;
        match &result.outcome {
            HookOutcome::Blocking { reason } => {
                assert_eq!(reason, "test");
            }
            other => panic!("Expected Blocking with reason 'test', got {:?}", other),
        }
    }

    #[test]
    fn test_parse_hook_output_valid_json() {
        let stdout = r#"some log line
{"decision":"block","reason":"policy violation"}
more output"#;
        let output = parse_hook_output(stdout);
        assert_eq!(output.decision.as_deref(), Some("block"));
        assert_eq!(output.reason.as_deref(), Some("policy violation"));
    }

    #[test]
    fn test_parse_hook_output_no_json() {
        let output = parse_hook_output("just plain text\nnothing here");
        assert!(output.decision.is_none());
        assert!(output.reason.is_none());
    }

    #[test]
    fn test_get_if_condition_command() {
        let hook = HookCommand::Command {
            command: "true".to_string(),
            shell: None,
            timeout: None,
            status_message: None,
            once: None,
            r#async: None,
            async_rewake: None,
            r#if: Some("Bash".to_string()),
        };
        assert_eq!(get_if_condition(&hook), Some("Bash".to_string()));
    }

    #[test]
    fn test_get_if_condition_none() {
        let hook = HookCommand::Command {
            command: "true".to_string(),
            shell: None,
            timeout: None,
            status_message: None,
            once: None,
            r#async: None,
            async_rewake: None,
            r#if: None,
        };
        assert_eq!(get_if_condition(&hook), None);
    }

    #[test]
    fn test_build_base_env() {
        let env = build_base_env("PreToolUse", Path::new("/tmp/test"));
        assert_eq!(env.get("ATOM_HOOK_TYPE").unwrap(), "PreToolUse");
        assert_eq!(env.get("ATOM_CWD").unwrap(), "/tmp/test");
        assert_eq!(env.get("ATOM_SESSION_ID").unwrap(), "");
        assert_eq!(env.get("ATOM_AGENT_ID").unwrap(), "");
    }

    #[tokio::test]
    async fn test_http_hooks_filtered_for_session_start() {
        let dir = TempDir::new().unwrap();
        let settings = settings_with_hook(
            "SessionStart",
            None,
            HookCommand::Http {
                url: "https://example.com/hook".to_string(),
                headers: None,
                allowed_env_vars: None,
                timeout: None,
                status_message: None,
                once: None,
                r#if: None,
            },
        );
        let result = execute_event_hooks(
            "SessionStart",
            "",
            &serde_json::json!({}),
            &settings,
            dir.path(),
        )
        .await;
        // HTTP hook should be filtered out, resulting in Success (no hooks executed)
        assert!(
            matches!(result.outcome, HookOutcome::Success),
            "Expected Success (HTTP hook filtered), got {:?}",
            result.outcome
        );
    }
}
