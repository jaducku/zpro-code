use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Provider connection template (api_base + api_key only, no models)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigJson {
    pub api_base: Option<String>,
    pub api_key: Option<String>,
}

/// LLM registration entry — references a provider or has inline connection info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfigJson {
    /// Reference to a provider in the `providers` map
    pub provider: Option<String>,
    /// Raw model ID sent to the API
    pub model_id: String,
    /// Inline connection info (when no provider reference)
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    /// Optional overrides
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u32>,
}

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

    /// Provider registry: named providers with their configs
    #[serde(default)]
    pub providers: Option<HashMap<String, ProviderConfigJson>>,

    /// Registered LLMs: name → config (provider ref or inline)
    #[serde(default)]
    pub llms: Option<HashMap<String, LlmConfigJson>>,

    /// Default provider name (key into providers map)
    #[serde(default)]
    pub default_provider: Option<String>,
}

impl AtomConfig {
    /// Load config from ~/.zpro-code/config.json
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

    #[test]
    fn test_parse_config_with_providers_and_llms() {
        let json = r#"{
            "providers": {
                "or": {
                    "api_base": "https://openrouter.ai/api/v1",
                    "api_key": "sk-or-test"
                }
            },
            "llms": {
                "sonnet": {
                    "provider": "or",
                    "model_id": "anthropic/claude-sonnet-4-20250514"
                },
                "qwen": {
                    "api_base": "http://localhost:8000/v1",
                    "model_id": "Qwen/Qwen2.5-72B"
                },
                "flash": {
                    "provider": "or",
                    "model_id": "google/gemini-2.5-flash",
                    "context_window": 1000000,
                    "max_output_tokens": 65536
                }
            },
            "default_model": "sonnet"
        }"#;
        let config: AtomConfig = serde_json::from_str(json).unwrap();

        let providers = config.providers.as_ref().unwrap();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers["or"].api_base.as_deref(), Some("https://openrouter.ai/api/v1"));

        let llms = config.llms.as_ref().unwrap();
        assert_eq!(llms.len(), 3);
        assert_eq!(llms["sonnet"].provider.as_deref(), Some("or"));
        assert_eq!(llms["sonnet"].model_id, "anthropic/claude-sonnet-4-20250514");
        assert_eq!(llms["qwen"].api_base.as_deref(), Some("http://localhost:8000/v1"));
        assert_eq!(llms["flash"].context_window, Some(1_000_000));
    }

    #[test]
    fn test_parse_config_without_providers_backward_compat() {
        let json = r#"{
            "provider": "openrouter",
            "api_key": "sk-test",
            "default_model": "fast",
            "models": { "fast": "google/gemma-3-27b-it:free" }
        }"#;
        let config: AtomConfig = serde_json::from_str(json).unwrap();
        assert!(config.providers.is_none());
        assert!(config.llms.is_none());
        assert_eq!(config.provider.as_deref(), Some("openrouter"));
        assert_eq!(
            config.default_model(),
            Some("google/gemma-3-27b-it:free".into())
        );
    }
}
