use std::path::Path;

use crate::services::hooks::types::HooksSettings;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::hooks::types::HookMatcher;

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
}
