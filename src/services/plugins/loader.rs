use std::collections::HashMap;

use crate::services::hooks::types::HooksSettings;

/// Definition of a built-in plugin that ships with the application.
#[derive(Debug, Clone, Default)]
pub struct BuiltinPluginDefinition {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub hooks: Option<HooksSettings>,
    pub default_enabled: Option<bool>,
}

impl BuiltinPluginDefinition {
    /// Returns the canonical plugin ID for a builtin plugin: `"{name}@builtin"`.
    pub fn plugin_id(&self) -> String {
        format!("{}@builtin", self.name)
    }
}

/// Registry holding all known builtin plugin definitions, keyed by name.
pub struct PluginRegistry {
    plugins: HashMap<String, BuiltinPluginDefinition>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, def: BuiltinPluginDefinition) {
        self.plugins.insert(def.name.clone(), def);
    }

    pub fn get(&self, name: &str) -> Option<&BuiltinPluginDefinition> {
        self.plugins.get(name)
    }

    pub fn all(&self) -> &HashMap<String, BuiltinPluginDefinition> {
        &self.plugins
    }

    /// Returns all plugins that are considered enabled given the provided settings.
    pub fn get_enabled(
        &self,
        enabled_settings: &HashMap<String, bool>,
    ) -> Vec<&BuiltinPluginDefinition> {
        self.plugins
            .values()
            .filter(|def| {
                is_plugin_enabled(&def.plugin_id(), enabled_settings, def.default_enabled)
            })
            .collect()
    }
}

/// Determine whether a plugin is enabled.
///
/// - If `enabled_settings` contains an explicit entry for `plugin_id`, use that value.
/// - Otherwise, fall back to `default_enabled`, treating `None` as `true`.
pub fn is_plugin_enabled(
    plugin_id: &str,
    enabled_settings: &HashMap<String, bool>,
    default_enabled: Option<bool>,
) -> bool {
    if let Some(&explicit) = enabled_settings.get(plugin_id) {
        return explicit;
    }
    default_enabled.unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_id_builtin() {
        let def = BuiltinPluginDefinition {
            name: "my-plugin".to_string(),
            ..Default::default()
        };
        assert_eq!(def.plugin_id(), "my-plugin@builtin");
    }

    #[test]
    fn test_is_plugin_enabled_default() {
        let settings = HashMap::new();
        // No explicit setting, default_enabled is None -> true
        assert!(is_plugin_enabled("foo@builtin", &settings, None));
        // No explicit setting, default_enabled is Some(true) -> true
        assert!(is_plugin_enabled("foo@builtin", &settings, Some(true)));
    }

    #[test]
    fn test_is_plugin_enabled_explicit_false() {
        let mut settings = HashMap::new();
        settings.insert("foo@builtin".to_string(), false);
        // Explicit false overrides default
        assert!(!is_plugin_enabled("foo@builtin", &settings, None));
        assert!(!is_plugin_enabled("foo@builtin", &settings, Some(true)));
    }

    #[test]
    fn test_register_and_get_builtin() {
        let mut registry = PluginRegistry::new();
        let def = BuiltinPluginDefinition {
            name: "test-plugin".to_string(),
            description: "A test plugin".to_string(),
            ..Default::default()
        };
        registry.register(def);

        let retrieved = registry.get("test-plugin");
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.name, "test-plugin");
        assert_eq!(retrieved.description, "A test plugin");

        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_get_enabled_plugins() {
        let mut registry = PluginRegistry::new();

        // Plugin A: default enabled (None -> true)
        registry.register(BuiltinPluginDefinition {
            name: "alpha".to_string(),
            description: "Alpha plugin".to_string(),
            default_enabled: None,
            ..Default::default()
        });

        // Plugin B: explicitly default disabled
        registry.register(BuiltinPluginDefinition {
            name: "beta".to_string(),
            description: "Beta plugin".to_string(),
            default_enabled: Some(false),
            ..Default::default()
        });

        // Plugin C: default enabled
        registry.register(BuiltinPluginDefinition {
            name: "gamma".to_string(),
            description: "Gamma plugin".to_string(),
            default_enabled: Some(true),
            ..Default::default()
        });

        // No overrides: alpha and gamma enabled, beta disabled
        let settings = HashMap::new();
        let enabled = registry.get_enabled(&settings);
        let names: Vec<&str> = enabled.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(!names.contains(&"beta"));
        assert!(names.contains(&"gamma"));
        assert_eq!(enabled.len(), 2);

        // Override: force beta on, force alpha off
        let mut settings = HashMap::new();
        settings.insert("beta@builtin".to_string(), true);
        settings.insert("alpha@builtin".to_string(), false);
        let enabled = registry.get_enabled(&settings);
        let names: Vec<&str> = enabled.iter().map(|d| d.name.as_str()).collect();
        assert!(!names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert!(names.contains(&"gamma"));
        assert_eq!(enabled.len(), 2);
    }
}
