use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Provider connection template with available models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Available models on this provider (for documentation/discovery).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
}

fn is_empty_map<K, V>(m: &HashMap<K, V>) -> bool {
    m.is_empty()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AtomConfig {
    /// Default model alias (key into models map).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    /// Provider registry: named providers with connection info + available models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub providers: Option<HashMap<String, ProviderConfigJson>>,

    /// Model aliases: alias → "provider:model_id" (e.g. "sonnet": "openrouter:anthropic/claude-sonnet-4-20250514").
    #[serde(default, skip_serializing_if = "is_empty_map")]
    pub models: HashMap<String, String>,

    // --- Legacy fields (backward compat, hidden when empty) ---

    /// Legacy v1: flat provider name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Legacy v1: flat API key
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Legacy v1: flat API base URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_base: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tavily_api_key: Option<String>,
}

impl AtomConfig {
    /// Load config from ~/.zpro-code/config.json.
    /// If no config file exists, creates a sample config first.
    pub fn load() -> Self {
        let path = crate::utils::paths::global_config_dir().join("config.json");
        if !path.exists() {
            let _ = Self::save_sample_to(&path);
        }
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Create a sample config with OpenRouter free models pre-registered.
    pub fn save_sample() -> anyhow::Result<()> {
        let path = crate::utils::paths::global_config_dir().join("config.json");
        Self::save_sample_to(&path)
    }

    pub fn save_sample_to(path: &Path) -> anyhow::Result<()> {
        let mut providers = HashMap::new();
        providers.insert(
            "openrouter".to_string(),
            ProviderConfigJson {
                api_base: Some("https://openrouter.ai/api/v1".to_string()),
                api_key: Some("YOUR_OPENROUTER_API_KEY".to_string()),
                models: vec![
                    "anthropic/claude-sonnet-4-20250514".to_string(),
                    "google/gemini-2.5-flash-preview-04-17".to_string(),
                    "google/gemma-3-27b-it:free".to_string(),
                ],
            },
        );

        let mut models = HashMap::new();
        models.insert("sonnet".into(), "openrouter:anthropic/claude-sonnet-4-20250514".into());
        models.insert("flash".into(), "openrouter:google/gemini-2.5-flash-preview-04-17".into());
        models.insert("free".into(), "openrouter:google/gemma-3-27b-it:free".into());

        let config = AtomConfig {
            default_model: Some("free".to_string()),
            providers: Some(providers),
            models,
            ..Default::default()
        };

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(&config)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = crate::utils::paths::global_config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("config.json");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Resolve a model alias → "provider:model_id" string.
    /// If not found in models map, returns as-is.
    pub fn resolve_model(&self, model_or_alias: &str) -> String {
        self.models
            .get(model_or_alias)
            .cloned()
            .unwrap_or_else(|| model_or_alias.to_string())
    }

    /// List all registered models as (alias, provider:model_id).
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
        assert!(config.providers.is_none());
        assert!(config.models.is_empty());
    }

    #[test]
    fn test_resolve_model_alias() {
        let mut config = AtomConfig::default();
        config.models.insert("fast".into(), "openrouter:google/gemma-3-27b-it:free".into());
        assert_eq!(config.resolve_model("fast"), "openrouter:google/gemma-3-27b-it:free");
        assert_eq!(config.resolve_model("unknown"), "unknown");
    }

    #[test]
    fn test_parse_v3_config() {
        let json = r#"{
            "default_model": "sonnet",
            "providers": {
                "openrouter": {
                    "api_base": "https://openrouter.ai/api/v1",
                    "api_key": "sk-test",
                    "models": [
                        "anthropic/claude-sonnet-4-20250514",
                        "google/gemma-3-27b-it:free"
                    ]
                },
                "local-mlx": {
                    "api_base": "http://localhost:8080/v1",
                    "models": [
                        "mlx-community/Qwen2.5-Coder-7B-Instruct-4bit"
                    ]
                }
            },
            "models": {
                "sonnet": "openrouter:anthropic/claude-sonnet-4-20250514",
                "free": "openrouter:google/gemma-3-27b-it:free",
                "qwen": "local-mlx:mlx-community/Qwen2.5-Coder-7B-Instruct-4bit"
            }
        }"#;
        let config: AtomConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.default_model.as_deref(), Some("sonnet"));

        let providers = config.providers.as_ref().unwrap();
        assert_eq!(providers.len(), 2);
        assert_eq!(providers["openrouter"].models.len(), 2);
        assert_eq!(providers["local-mlx"].models.len(), 1);

        assert_eq!(config.models.len(), 3);
        assert_eq!(config.models["sonnet"], "openrouter:anthropic/claude-sonnet-4-20250514");
        assert_eq!(config.models["qwen"], "local-mlx:mlx-community/Qwen2.5-Coder-7B-Instruct-4bit");

        assert_eq!(
            config.resolve_model("sonnet"),
            "openrouter:anthropic/claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_load_missing_file() {
        let config = AtomConfig::load_from(Path::new("/nonexistent/path/config.json"));
        assert!(config.providers.is_none());
        assert!(config.models.is_empty());
    }

    #[test]
    fn test_save_sample_creates_valid_v3_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.json");
        AtomConfig::save_sample_to(&path).unwrap();

        assert!(path.exists());
        let config = AtomConfig::load_from(&path);

        let providers = config.providers.as_ref().expect("providers should exist");
        assert!(providers.contains_key("openrouter"));
        assert_eq!(providers["openrouter"].models.len(), 3);

        assert_eq!(config.models.len(), 3);
        assert!(config.models["sonnet"].starts_with("openrouter:"));
        assert!(config.models["free"].starts_with("openrouter:"));

        assert_eq!(config.default_model.as_deref(), Some("free"));

        // v1 fields should NOT be populated
        assert!(config.provider.is_none());
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_v1_backward_compat_still_parses() {
        let json = r#"{
            "provider": "openrouter",
            "api_key": "sk-test",
            "default_model": "fast",
            "models": { "fast": "google/gemma-3-27b-it:free" }
        }"#;
        let config: AtomConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.provider.as_deref(), Some("openrouter"));
        assert_eq!(config.models["fast"], "google/gemma-3-27b-it:free");
    }
}
