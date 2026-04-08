use std::collections::HashMap;

use super::loader::{BuiltinPluginDefinition, PluginRegistry};
use crate::services::hooks::types::HooksSettings;
#[cfg(test)]
use crate::services::hooks::types::{HookCommand, HookMatcher};

/// Merge a plugin's hooks into the base hooks settings.
///
/// For each event in the plugin's hooks, the matchers are appended to the
/// corresponding event entry in `base`.
pub fn merge_plugin_hooks(base: &mut HooksSettings, plugin: &BuiltinPluginDefinition) {
    if let Some(ref plugin_hooks) = plugin.hooks {
        for (event, matchers) in plugin_hooks {
            base.entry(event.clone())
                .or_default()
                .extend(matchers.clone());
        }
    }
}

/// Collect hooks from all enabled plugins into a single `HooksSettings`.
pub fn collect_plugin_hooks(
    registry: &PluginRegistry,
    enabled_settings: &HashMap<String, bool>,
) -> HooksSettings {
    let mut result = HooksSettings::new();
    for plugin in registry.get_enabled(enabled_settings) {
        merge_plugin_hooks(&mut result, plugin);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_plugin_hooks_into_settings() {
        let mut base = HooksSettings::new();

        let mut plugin_hooks = HooksSettings::new();
        plugin_hooks.insert(
            "SessionStart".to_string(),
            vec![HookMatcher {
                matcher: None,
                hooks: vec![HookCommand::Command {
                    command: "echo hello".to_string(),
                    shell: None,
                    timeout: None,
                    status_message: None,
                    once: None,
                    r#async: None,
                    async_rewake: None,
                    r#if: None,
                }],
            }],
        );

        let plugin = BuiltinPluginDefinition {
            name: "test-plugin".to_string(),
            description: "A test plugin".to_string(),
            hooks: Some(plugin_hooks),
            ..Default::default()
        };

        merge_plugin_hooks(&mut base, &plugin);

        assert!(base.contains_key("SessionStart"));
        let matchers = &base["SessionStart"];
        assert_eq!(matchers.len(), 1);
        assert!(matchers[0].matcher.is_none());
        assert_eq!(matchers[0].hooks.len(), 1);
    }

    #[test]
    fn test_merge_plugin_hooks_no_hooks() {
        let mut base = HooksSettings::new();
        base.insert(
            "Stop".to_string(),
            vec![HookMatcher {
                matcher: Some("*.rs".to_string()),
                hooks: vec![],
            }],
        );

        let plugin = BuiltinPluginDefinition {
            name: "empty-plugin".to_string(),
            description: "No hooks".to_string(),
            hooks: None,
            ..Default::default()
        };

        merge_plugin_hooks(&mut base, &plugin);

        // Base is unchanged
        assert_eq!(base.len(), 1);
        assert!(base.contains_key("Stop"));
        assert_eq!(base["Stop"].len(), 1);
    }
}
