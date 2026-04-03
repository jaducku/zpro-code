pub mod async_registry;
pub mod command_hook;
pub mod compat;
pub mod executor;
pub mod http_hook;
pub mod matcher;
pub mod settings;
pub mod types;

pub use types::*;
pub use executor::execute_event_hooks;
pub use settings::load_hooks_from_settings;
pub use compat::load_legacy_hooks;

use std::path::Path;

/// Load all hooks: new settings format + legacy fallback.
pub fn load_all_hooks(cwd: &Path) -> HooksSettings {
    let mut all_settings = load_hooks_from_settings(cwd);
    let legacy = load_legacy_hooks(cwd);
    for (event, matchers) in legacy {
        all_settings.entry(event).or_default().extend(matchers);
    }
    all_settings
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_all_hooks_empty() {
        let dir = TempDir::new().unwrap();
        let hooks = load_all_hooks(dir.path());
        assert!(hooks.is_empty());
    }

    #[test]
    fn test_load_all_hooks_legacy_only() {
        let dir = TempDir::new().unwrap();
        let agents_dir = dir.path().join(".agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("hooks.json"),
            r#"[{"hook_type":"PreToolUse","tool_name":null,"command":"echo ok"}]"#,
        )
        .unwrap();
        let hooks = load_all_hooks(dir.path());
        assert!(hooks.contains_key("PreToolUse"));
        assert_eq!(hooks["PreToolUse"].len(), 1);
    }

    #[test]
    fn test_load_all_hooks_settings_only() {
        let dir = TempDir::new().unwrap();
        let agents_dir = dir.path().join(".agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(
            agents_dir.join("settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo hi"}]}]}}"#,
        )
        .unwrap();
        let hooks = load_all_hooks(dir.path());
        assert!(hooks.contains_key("PreToolUse"));
        assert_eq!(hooks["PreToolUse"].len(), 1);
    }

    #[test]
    fn test_load_all_hooks_merges_both() {
        let dir = TempDir::new().unwrap();
        let agents_dir = dir.path().join(".agents");
        std::fs::create_dir_all(&agents_dir).unwrap();

        // Settings format
        std::fs::write(
            agents_dir.join("settings.json"),
            r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo settings"}]}]}}"#,
        )
        .unwrap();

        // Legacy format
        std::fs::write(
            agents_dir.join("hooks.json"),
            r#"[{"hook_type":"PreToolUse","tool_name":"Edit","command":"echo legacy"}]"#,
        )
        .unwrap();

        let hooks = load_all_hooks(dir.path());
        assert!(hooks.contains_key("PreToolUse"));
        // Should have 2 matchers: one from settings, one from legacy
        assert_eq!(hooks["PreToolUse"].len(), 2);
    }
}
