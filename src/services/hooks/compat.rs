use serde::Deserialize;
use std::path::Path;

use crate::services::hooks::settings::merge_hooks_settings;
use crate::services::hooks::types::{HookCommand, HookMatcher, HooksSettings};

/// Legacy hook configuration format (flat JSON array entries).
#[derive(Debug, Clone, Deserialize)]
pub struct LegacyHookConfig {
    pub hook_type: String,
    pub tool_name: Option<String>,
    pub command: String,
}

/// Convert a slice of legacy hook configs into the new HooksSettings format.
///
/// Each legacy entry becomes a HookMatcher with the tool_name as matcher
/// and a single Command-type HookCommand.
pub fn convert_legacy_to_settings(legacy: &[LegacyHookConfig]) -> HooksSettings {
    let mut settings = HooksSettings::new();

    for entry in legacy {
        let hook_command = HookCommand::Command {
            command: entry.command.clone(),
            shell: None,
            timeout: None,
            status_message: None,
            once: None,
            r#async: None,
            async_rewake: None,
            r#if: None,
        };

        let matcher = HookMatcher {
            matcher: entry.tool_name.clone(),
            hooks: vec![hook_command],
        };

        settings
            .entry(entry.hook_type.clone())
            .or_insert_with(Vec::new)
            .push(matcher);
    }

    settings
}

/// Load legacy hooks from hooks.json files (project-level then user-level).
///
/// - Project: `{cwd}/.agents/hooks.json`
/// - User: `~/.zpro-code/hooks.json`
pub fn load_legacy_hooks(cwd: &Path) -> HooksSettings {
    let mut result = HooksSettings::new();

    // Project-level
    let project_hooks = cwd.join(".agents/hooks.json");
    if let Some(hooks) = read_legacy_file(&project_hooks) {
        merge_hooks_settings(&mut result, hooks);
    }

    // User-level
    let user_hooks = crate::utils::paths::global_config_dir().join("hooks.json");
    if let Some(hooks) = read_legacy_file(&user_hooks) {
        merge_hooks_settings(&mut result, hooks);
    }

    result
}

/// Read and convert a single legacy hooks.json file.
fn read_legacy_file(path: &Path) -> Option<HooksSettings> {
    let content = std::fs::read_to_string(path).ok()?;
    let legacy: Vec<LegacyHookConfig> = serde_json::from_str(&content).ok()?;
    Some(convert_legacy_to_settings(&legacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_legacy_hooks_to_matchers() {
        let legacy = vec![
            LegacyHookConfig {
                hook_type: "PreToolUse".to_string(),
                tool_name: Some("Bash".to_string()),
                command: "echo pre-bash".to_string(),
            },
            LegacyHookConfig {
                hook_type: "PreToolUse".to_string(),
                tool_name: None,
                command: "echo pre-all".to_string(),
            },
            LegacyHookConfig {
                hook_type: "PostToolUse".to_string(),
                tool_name: Some("Edit".to_string()),
                command: "echo post-edit".to_string(),
            },
        ];

        let settings = convert_legacy_to_settings(&legacy);

        // Two PreToolUse matchers
        let pre = &settings["PreToolUse"];
        assert_eq!(pre.len(), 2);
        assert_eq!(pre[0].matcher.as_deref(), Some("Bash"));
        assert_eq!(pre[1].matcher, None);

        // One PostToolUse matcher
        let post = &settings["PostToolUse"];
        assert_eq!(post.len(), 1);
        assert_eq!(post[0].matcher.as_deref(), Some("Edit"));

        // Verify the command inside the first matcher
        match &pre[0].hooks[0] {
            HookCommand::Command { command, .. } => {
                assert_eq!(command, "echo pre-bash");
            }
            _ => panic!("Expected Command variant"),
        }
    }

    #[test]
    fn test_load_legacy_hooks_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let agents_dir = dir.path().join(".agents");
        std::fs::create_dir_all(&agents_dir).unwrap();

        let hooks_json = r#"[
            {"hook_type": "PreToolUse", "tool_name": "Bash", "command": "echo ok"},
            {"hook_type": "PostToolUse", "tool_name": null, "command": "echo done"}
        ]"#;
        std::fs::write(agents_dir.join("hooks.json"), hooks_json).unwrap();

        let settings = load_legacy_hooks(dir.path());

        assert!(settings.contains_key("PreToolUse"));
        assert!(settings.contains_key("PostToolUse"));
        assert_eq!(settings["PreToolUse"].len(), 1);
        assert_eq!(settings["PreToolUse"][0].matcher.as_deref(), Some("Bash"));
        assert_eq!(settings["PostToolUse"].len(), 1);
        assert_eq!(settings["PostToolUse"][0].matcher, None);
    }

    #[test]
    fn test_load_legacy_hooks_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let settings = load_legacy_hooks(dir.path());
        assert!(settings.is_empty());
    }
}
