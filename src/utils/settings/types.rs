use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsJson {
    #[serde(rename = "$schema")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_helper: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub aws_credential_export: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gcp_auth_refresh: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_suggestion: Option<FileSuggestion>,

    #[serde(default = "default_true")]
    pub respect_gitignore: bool, // NOTE: custom Default impl below sets this to true

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cleanup_period_days: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<Attribution>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_models: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mcp_servers: Option<Vec<McpServerEntry>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub denied_mcp_servers: Option<Vec<McpServerEntry>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<WorktreeConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_mode: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_thinking_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_all_project_mcp_servers: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_memory_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_memory_directory: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_style: Option<String>,

    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_plugins: Option<HashMap<String, bool>>,

    /// Passthrough for unknown fields
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn default_true() -> bool {
    true
}

impl Default for SettingsJson {
    fn default() -> Self {
        Self {
            respect_gitignore: true, // intentionally true, matching serde default
            schema: None,
            api_key_helper: None,
            aws_credential_export: None,
            gcp_auth_refresh: None,
            file_suggestion: None,
            cleanup_period_days: None,
            env: None,
            attribution: None,
            permissions: None,
            model: None,
            available_models: None,
            allowed_mcp_servers: None,
            denied_mcp_servers: None,
            hooks: None,
            worktree: None,
            agent: None,
            fast_mode: None,
            always_thinking_enabled: None,
            enable_all_project_mcp_servers: None,
            auto_memory_enabled: None,
            auto_memory_directory: None,
            output_style: None,
            enabled_plugins: None,
            extra: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSuggestion {
    #[serde(rename = "type")]
    pub suggestion_type: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribution {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Vec<crate::types::permissions::PermissionRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Vec<crate::types::permissions::PermissionRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<Vec<crate::types::permissions::PermissionRule>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_directories: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sparse_paths: Option<Vec<String>>,
}

/// Where a setting value came from
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsSource {
    Default,
    GlobalFile,
    ProjectFile,
    ManagedPolicy,
    Environment,
    CliFlag,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_default() {
        let settings = SettingsJson::default();
        assert!(settings.respect_gitignore); // Custom Default impl sets true
    }

    #[test]
    fn test_settings_parse_minimal() {
        let json = r#"{}"#;
        let settings: SettingsJson = serde_json::from_str(json).unwrap();
        assert!(settings.model.is_none());
        assert!(settings.extra.is_empty());
    }

    #[test]
    fn test_settings_parse_with_model() {
        let json = r#"{"model": "claude-sonnet-4-20250514"}"#;
        let settings: SettingsJson = serde_json::from_str(json).unwrap();
        assert_eq!(settings.model.as_deref(), Some("claude-sonnet-4-20250514"));
    }

    #[test]
    fn test_settings_passthrough_unknown_fields() {
        let json = r#"{"model": "test", "customField": 42}"#;
        let settings: SettingsJson = serde_json::from_str(json).unwrap();
        assert_eq!(settings.extra["customField"], 42);
    }

    #[test]
    fn test_settings_roundtrip() {
        let settings = SettingsJson {
            model: Some("test-model".into()),
            fast_mode: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: SettingsJson = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.model, settings.model);
        assert_eq!(parsed.fast_mode, settings.fast_mode);
    }

    #[test]
    fn test_mcp_server_entry() {
        let json = r#"{"serverName": "test", "serverCommand": ["npx", "mcp-server"]}"#;
        let entry: McpServerEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.server_name.as_deref(), Some("test"));
        assert_eq!(entry.server_command.as_ref().unwrap().len(), 2);
    }
}
