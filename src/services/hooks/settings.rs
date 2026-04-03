use std::collections::HashSet;
use std::path::Path;

use crate::services::hooks::types::{HookCommand, HookMatcher, HooksSettings};

/// Read and parse the "hooks" key from a single settings JSON file.
/// Returns `None` if the file doesn't exist or doesn't contain a valid "hooks" key.
pub fn read_hooks_from_file(path: &Path) -> Option<HooksSettings> {
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    let hooks_value = parsed.get("hooks")?;
    serde_json::from_value::<HooksSettings>(hooks_value.clone()).ok()
}

/// Merge overlay hooks into base. For each event key, extend the matchers vec.
pub fn merge_hooks_settings(base: &mut HooksSettings, overlay: HooksSettings) {
    for (event, matchers) in overlay {
        base.entry(event).or_insert_with(Vec::new).extend(matchers);
    }
}

/// Load hooks from settings.json files (project-level then user-level).
///
/// - Project: `{cwd}/.agents/settings.json` -> "hooks" key
/// - User: `~/.atom/settings.json` -> "hooks" key
pub fn load_hooks_from_settings(cwd: &Path) -> HooksSettings {
    let mut result = HooksSettings::new();

    // Project-level
    let project_settings = cwd.join(".agents/settings.json");
    if let Some(hooks) = read_hooks_from_file(&project_settings) {
        merge_hooks_settings(&mut result, hooks);
    }

    // User-level
    let user_settings = crate::utils::paths::global_config_dir().join("settings.json");
    if let Some(hooks) = read_hooks_from_file(&user_settings) {
        merge_hooks_settings(&mut result, hooks);
    }

    result
}

/// Compute a deduplication key for a hook within a given matcher.
///
/// For Command: `"{shell}\0{command}\0{if_condition}"`
/// For other types: `"{type}\0{url_or_prompt}\0{if_condition}"`
pub fn hook_dedup_key(matcher: &HookMatcher, hook: &HookCommand) -> String {
    let _ = matcher; // matcher context reserved for future use
    match hook {
        HookCommand::Command {
            command,
            shell,
            r#if,
            ..
        } => format!(
            "{}\0{}\0{}",
            shell.as_deref().unwrap_or(""),
            command,
            r#if.as_deref().unwrap_or("")
        ),
        HookCommand::Prompt { prompt, r#if, .. } => {
            format!("prompt\0{}\0{}", prompt, r#if.as_deref().unwrap_or(""))
        }
        HookCommand::Agent { prompt, r#if, .. } => {
            format!("agent\0{}\0{}", prompt, r#if.as_deref().unwrap_or(""))
        }
        HookCommand::Http { url, r#if, .. } => {
            format!("http\0{}\0{}", url, r#if.as_deref().unwrap_or(""))
        }
    }
}

/// Deduplicate hooks within each event's matchers.
///
/// For each event, walks all matchers and their hooks, removing duplicates
/// based on `hook_dedup_key`. Different shells or if-conditions are considered
/// distinct (not duplicates).
pub fn dedup_hooks_settings(settings: HooksSettings) -> HooksSettings {
    let mut result = HooksSettings::new();

    for (event, matchers) in settings {
        let mut seen = HashSet::new();
        let mut deduped_matchers = Vec::new();

        for matcher in matchers {
            let mut deduped_hooks = Vec::new();
            for hook in &matcher.hooks {
                let key = hook_dedup_key(&matcher, hook);
                if seen.insert(key) {
                    deduped_hooks.push(hook.clone());
                }
            }
            if !deduped_hooks.is_empty() {
                deduped_matchers.push(HookMatcher {
                    matcher: matcher.matcher.clone(),
                    hooks: deduped_hooks,
                });
            }
        }

        if !deduped_matchers.is_empty() {
            result.insert(event, deduped_matchers);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_hooks_settings_from_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let agents_dir = dir.path().join(".agents");
        std::fs::create_dir_all(&agents_dir).unwrap();

        let settings_json = r#"{
            "hooks": {
                "PreToolUse": [
                    {
                        "matcher": "Bash",
                        "hooks": [
                            {
                                "type": "command",
                                "command": "echo pre-tool"
                            }
                        ]
                    }
                ]
            }
        }"#;
        std::fs::write(agents_dir.join("settings.json"), settings_json).unwrap();

        let hooks = load_hooks_from_settings(dir.path());
        assert!(hooks.contains_key("PreToolUse"));
        let matchers = &hooks["PreToolUse"];
        assert_eq!(matchers.len(), 1);
        assert_eq!(matchers[0].matcher.as_deref(), Some("Bash"));
        assert_eq!(matchers[0].hooks.len(), 1);
    }

    #[test]
    fn test_load_hooks_settings_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let hooks = load_hooks_from_settings(dir.path());
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_read_hooks_from_file_no_hooks_key() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"theme": "dark"}"#).unwrap();
        assert!(read_hooks_from_file(&path).is_none());
    }

    #[test]
    fn test_merge_hooks_settings() {
        let mut base = HooksSettings::new();
        base.insert(
            "PreToolUse".to_string(),
            vec![HookMatcher {
                matcher: Some("Bash".to_string()),
                hooks: vec![],
            }],
        );

        let mut overlay = HooksSettings::new();
        overlay.insert(
            "PreToolUse".to_string(),
            vec![HookMatcher {
                matcher: Some("Edit".to_string()),
                hooks: vec![],
            }],
        );
        overlay.insert(
            "PostToolUse".to_string(),
            vec![HookMatcher {
                matcher: None,
                hooks: vec![],
            }],
        );

        merge_hooks_settings(&mut base, overlay);

        assert_eq!(base["PreToolUse"].len(), 2);
        assert_eq!(base["PostToolUse"].len(), 1);
    }

    fn make_command_hook(command: &str, shell: Option<&str>, if_cond: Option<&str>) -> HookCommand {
        HookCommand::Command {
            command: command.to_string(),
            shell: shell.map(|s| s.to_string()),
            timeout: None,
            status_message: None,
            once: None,
            r#async: None,
            async_rewake: None,
            r#if: if_cond.map(|s| s.to_string()),
        }
    }

    #[test]
    fn test_dedup_hooks_removes_duplicates() {
        let mut settings = HooksSettings::new();
        settings.insert(
            "PreToolUse".to_string(),
            vec![HookMatcher {
                matcher: None,
                hooks: vec![
                    make_command_hook("echo hello", None, None),
                    make_command_hook("echo hello", None, None),
                ],
            }],
        );

        let deduped = dedup_hooks_settings(settings);
        let hooks = &deduped["PreToolUse"][0].hooks;
        assert_eq!(hooks.len(), 1);
    }

    #[test]
    fn test_dedup_hooks_keeps_different_shells() {
        let mut settings = HooksSettings::new();
        settings.insert(
            "PreToolUse".to_string(),
            vec![HookMatcher {
                matcher: None,
                hooks: vec![
                    make_command_hook("echo hello", Some("bash"), None),
                    make_command_hook("echo hello", Some("zsh"), None),
                ],
            }],
        );

        let deduped = dedup_hooks_settings(settings);
        let hooks = &deduped["PreToolUse"][0].hooks;
        assert_eq!(hooks.len(), 2);
    }

    #[test]
    fn test_dedup_hooks_keeps_different_if() {
        let mut settings = HooksSettings::new();
        settings.insert(
            "PreToolUse".to_string(),
            vec![HookMatcher {
                matcher: None,
                hooks: vec![
                    make_command_hook("echo hello", None, Some("Bash")),
                    make_command_hook("echo hello", None, Some("Edit")),
                ],
            }],
        );

        let deduped = dedup_hooks_settings(settings);
        let hooks = &deduped["PreToolUse"][0].hooks;
        assert_eq!(hooks.len(), 2);
    }
}
