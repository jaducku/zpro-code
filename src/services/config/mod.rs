use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AtomConfig {
    /// Provider: "openrouter", "codex", "openai", "ollama", "custom"
    #[serde(default)]
    pub provider: Option<String>,

    /// API key
    #[serde(default)]
    pub api_key: Option<String>,

    /// Custom API base URL (only needed for "custom" provider)
    #[serde(default)]
    pub api_base: Option<String>,

    /// Default model alias (key into models map)
    #[serde(default)]
    pub default_model: Option<String>,

    /// Named model aliases: { "fast": "google/gemma-3-27b-it:free", ... }
    #[serde(default)]
    pub models: HashMap<String, String>,

    /// Default permission mode
    #[serde(default)]
    pub permission_mode: Option<String>,

    /// Tavily API key for web search
    #[serde(default)]
    pub tavily_api_key: Option<String>,
}

impl AtomConfig {
    /// Load config from ~/.atom/config.json
    pub fn load() -> Self {
        let path = crate::utils::paths::global_config_dir().join("config.json");
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = crate::utils::paths::global_config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("config.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Resolve a model string -- if it's an alias in the models map, expand it.
    /// Otherwise return as-is (it's a full model ID).
    pub fn resolve_model(&self, model_or_alias: &str) -> String {
        self.models
            .get(model_or_alias)
            .cloned()
            .unwrap_or_else(|| model_or_alias.to_string())
    }

    /// Get the default model (resolved from alias if needed)
    pub fn default_model(&self) -> Option<String> {
        self.default_model.as_ref().map(|m| self.resolve_model(m))
    }

    /// Get API base URL based on provider name
    pub fn api_base_for_provider(&self) -> Option<String> {
        match self.provider.as_deref() {
            Some("openrouter") => Some("https://openrouter.ai/api/v1".into()),
            Some("openai") => Some("https://api.openai.com/v1".into()),
            Some("ollama") => Some("http://localhost:11434/v1".into()),
            Some("codex") => None, // handled specially by codex auth
            Some("custom") => self.api_base.clone(),
            _ => self.api_base.clone(),
        }
    }

    /// List all registered models as (alias, full_model_id)
    pub fn list_models(&self) -> Vec<(String, String)> {
        let mut models: Vec<_> = self
            .models
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        models.sort_by(|a, b| a.0.cmp(&b.0));
        models
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AtomConfig::default();
        assert!(config.provider.is_none());
        assert!(config.models.is_empty());
    }

    #[test]
    fn test_resolve_model_alias() {
        let mut config = AtomConfig::default();
        config
            .models
            .insert("fast".into(), "google/gemma-3-27b-it:free".into());
        assert_eq!(config.resolve_model("fast"), "google/gemma-3-27b-it:free");
        assert_eq!(config.resolve_model("unknown-model"), "unknown-model");
    }

    #[test]
    fn test_api_base_for_provider() {
        let mut config = AtomConfig::default();
        config.provider = Some("openrouter".into());
        assert_eq!(
            config.api_base_for_provider().unwrap(),
            "https://openrouter.ai/api/v1"
        );

        config.provider = Some("ollama".into());
        assert_eq!(
            config.api_base_for_provider().unwrap(),
            "http://localhost:11434/v1"
        );
    }

    #[test]
    fn test_parse_config_json() {
        let json = r#"{
            "provider": "openrouter",
            "api_key": "sk-test",
            "default_model": "fast",
            "models": {
                "fast": "google/gemma-3-27b-it:free",
                "smart": "anthropic/claude-sonnet-4-20250514"
            }
        }"#;
        let config: AtomConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.provider.as_deref(), Some("openrouter"));
        assert_eq!(
            config.default_model(),
            Some("google/gemma-3-27b-it:free".into())
        );
        assert_eq!(config.list_models().len(), 2);
    }

    #[test]
    fn test_load_missing_file() {
        let config = AtomConfig::load_from(std::path::Path::new("/nonexistent/path/config.json"));
        assert!(config.provider.is_none());
        assert!(config.models.is_empty());
    }
}
