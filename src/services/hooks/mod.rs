pub mod command_hook;
pub mod compat;
pub mod matcher;
pub mod settings;
pub mod types;

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Hook types that can fire during tool execution
#[derive(Debug, Clone, PartialEq)]
pub enum HookType {
    PreToolUse,
    PostToolUse,
}

/// A configured hook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookConfig {
    pub hook_type: String,
    pub tool_name: Option<String>,
    pub command: String,
}

/// Result from running a hook
#[derive(Debug)]
pub enum HookResult {
    /// Hook passed, continue execution
    Allow,
    /// Hook wants to block this tool call
    Deny { reason: String },
    /// Hook modified the output (PostToolUse only)
    Modified { output: String },
    /// Hook failed to run
    Error { message: String },
}

/// Execute hooks of a given type for a tool call.
/// Returns the combined result (any Deny blocks the operation).
pub fn execute_hooks(
    hook_type: HookType,
    tool_name: &str,
    tool_input: &serde_json::Value,
    tool_output: Option<&str>,
    hooks: &[HookConfig],
    cwd: &Path,
) -> HookResult {
    let type_str = match hook_type {
        HookType::PreToolUse => "PreToolUse",
        HookType::PostToolUse => "PostToolUse",
    };

    let matching_hooks: Vec<_> = hooks
        .iter()
        .filter(|h| h.hook_type == type_str)
        .filter(|h| h.tool_name.as_ref().is_none_or(|n| n == tool_name))
        .collect();

    if matching_hooks.is_empty() {
        return HookResult::Allow;
    }

    for hook in matching_hooks {
        let input_json = serde_json::to_string(tool_input).unwrap_or_default();

        let result = Command::new("sh")
            .arg("-c")
            .arg(&hook.command)
            .current_dir(cwd)
            .env("HOOK_TYPE", type_str)
            .env("TOOL_NAME", tool_name)
            .env("TOOL_INPUT", &input_json)
            .env("TOOL_OUTPUT", tool_output.unwrap_or(""))
            .output();

        match result {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    let reason = if !stderr.is_empty() {
                        stderr
                    } else {
                        format!(
                            "Hook '{}' exited with code {}",
                            hook.command,
                            output.status.code().unwrap_or(-1)
                        )
                    };
                    return HookResult::Deny { reason };
                }

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    if json.get("deny").and_then(|v| v.as_bool()).unwrap_or(false) {
                        let reason = json
                            .get("reason")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Denied by hook")
                            .to_string();
                        return HookResult::Deny { reason };
                    }
                    if let Some(output) = json.get("output").and_then(|v| v.as_str()) {
                        return HookResult::Modified {
                            output: output.to_string(),
                        };
                    }
                }
            }
            Err(e) => {
                return HookResult::Error {
                    message: format!("Hook failed: {}", e),
                };
            }
        }
    }

    HookResult::Allow
}

/// Load hooks from project-level and user-level config files.
pub fn load_hooks(cwd: &Path) -> Vec<HookConfig> {
    let mut hooks = Vec::new();

    // Check project-level .agents/hooks.json
    let project_hooks = cwd.join(".agents/hooks.json");
    if let Ok(content) = std::fs::read_to_string(&project_hooks) {
        if let Ok(parsed) = serde_json::from_str::<Vec<HookConfig>>(&content) {
            hooks.extend(parsed);
        }
    }

    // Check user-level
    {
        let user_hooks = crate::utils::paths::user_hooks_path();
        if let Ok(content) = std::fs::read_to_string(&user_hooks) {
            if let Ok(parsed) = serde_json::from_str::<Vec<HookConfig>>(&content) {
                hooks.extend(parsed);
            }
        }
    }

    hooks
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_no_hooks_allows() {
        let dir = TempDir::new().unwrap();
        let result = execute_hooks(
            HookType::PreToolUse,
            "Bash",
            &serde_json::json!({}),
            None,
            &[],
            dir.path(),
        );
        assert!(matches!(result, HookResult::Allow));
    }

    #[test]
    fn test_hook_allow_on_success() {
        let dir = TempDir::new().unwrap();
        let hooks = vec![HookConfig {
            hook_type: "PreToolUse".into(),
            tool_name: None,
            command: "true".into(),
        }];
        let result = execute_hooks(
            HookType::PreToolUse,
            "Bash",
            &serde_json::json!({}),
            None,
            &hooks,
            dir.path(),
        );
        assert!(matches!(result, HookResult::Allow));
    }

    #[test]
    fn test_hook_deny_on_failure() {
        let dir = TempDir::new().unwrap();
        let hooks = vec![HookConfig {
            hook_type: "PreToolUse".into(),
            tool_name: None,
            command: "false".into(),
        }];
        let result = execute_hooks(
            HookType::PreToolUse,
            "Bash",
            &serde_json::json!({}),
            None,
            &hooks,
            dir.path(),
        );
        assert!(matches!(result, HookResult::Deny { .. }));
    }

    #[test]
    fn test_hook_filters_by_tool_name() {
        let dir = TempDir::new().unwrap();
        let hooks = vec![HookConfig {
            hook_type: "PreToolUse".into(),
            tool_name: Some("Edit".into()),
            command: "false".into(),
        }];
        // Should not match Bash
        let result = execute_hooks(
            HookType::PreToolUse,
            "Bash",
            &serde_json::json!({}),
            None,
            &hooks,
            dir.path(),
        );
        assert!(matches!(result, HookResult::Allow));
    }

    #[test]
    fn test_hook_json_deny() {
        let dir = TempDir::new().unwrap();
        let hooks = vec![HookConfig {
            hook_type: "PreToolUse".into(),
            tool_name: None,
            command: r#"echo '{"deny":true,"reason":"blocked by policy"}'"#.into(),
        }];
        let result = execute_hooks(
            HookType::PreToolUse,
            "Bash",
            &serde_json::json!({}),
            None,
            &hooks,
            dir.path(),
        );
        match result {
            HookResult::Deny { reason } => assert!(reason.contains("blocked by policy")),
            _ => panic!("Expected Deny"),
        }
    }

    #[test]
    fn test_load_hooks_empty() {
        let dir = TempDir::new().unwrap();
        let hooks = load_hooks(dir.path());
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_load_hooks_from_file() {
        let dir = TempDir::new().unwrap();
        let agents_dir = dir.path().join(".agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("hooks.json"),
            r#"[{"hook_type":"PreToolUse","tool_name":null,"command":"echo ok"}]"#,
        )
        .unwrap();
        let hooks = load_hooks(dir.path());
        assert_eq!(hooks.len(), 1);
    }
}
