use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::openai_provider::ChatCompletionsProvider;
use super::provider::LlmProvider;
use super::types::{Capabilities, ModelInfo};

/// Resolved model entry with optional overrides
#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: String,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u32>,
}

/// Runtime provider configuration
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider_type: String,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub models: HashMap<String, ModelEntry>,
}

/// Multi-provider registry with lazy creation and caching.
pub struct ProviderRegistry {
    configs: HashMap<String, ProviderConfig>,
    cache: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
    default_provider: String,
    default_model: Option<String>,
}

impl ProviderRegistry {
    pub fn new(default_provider: String, default_model: Option<String>) -> Self {
        Self {
            configs: HashMap::new(),
            cache: RwLock::new(HashMap::new()),
            default_provider,
            default_model,
        }
    }

    /// Build from AtomConfig.
    pub fn from_config(config: &crate::services::config::AtomConfig) -> anyhow::Result<Self> {
        use crate::services::config::ModelEntryJson;

        let default_provider = config.default_provider.clone()
            .unwrap_or_else(|| "_cli".into());
        let default_model = config.default_model.clone();

        let mut registry = Self::new(default_provider, default_model);

        if let Some(ref providers) = config.providers {
            for (name, pcj) in providers {
                if name.starts_with('_') {
                    anyhow::bail!("Provider name '{}' uses reserved '_' prefix", name);
                }
                let mut models = HashMap::new();
                if let Some(ref model_map) = pcj.models {
                    for (alias, entry_json) in model_map {
                        let entry = match entry_json {
                            ModelEntryJson::Short(id) => ModelEntry {
                                id: id.clone(),
                                context_window: None,
                                max_output_tokens: None,
                            },
                            ModelEntryJson::Full { id, context_window, max_output_tokens } => ModelEntry {
                                id: id.clone(),
                                context_window: *context_window,
                                max_output_tokens: *max_output_tokens,
                            },
                        };
                        models.insert(alias.clone(), entry);
                    }
                }
                registry.register(name.clone(), ProviderConfig {
                    provider_type: pcj.provider_type.clone().unwrap_or_else(|| "openai".into()),
                    api_base: pcj.api_base.clone(),
                    api_key: pcj.api_key.clone(),
                    models,
                });
            }
        }

        Ok(registry)
    }

    /// Register a provider config. Must be called before wrapping in Arc.
    pub fn register(&mut self, name: String, config: ProviderConfig) {
        self.configs.insert(name, config);
    }

    /// Set the default provider and model.
    pub fn set_default(&mut self, provider: String, model: Option<String>) {
        self.default_provider = provider;
        if model.is_some() {
            self.default_model = model;
        }
    }

    /// Get the default provider+model.
    pub fn default_provider(&self) -> anyhow::Result<Arc<dyn LlmProvider>> {
        let model = self.default_model.as_deref()
            .ok_or_else(|| anyhow::anyhow!("No default model configured"))?;
        self.resolve(model)
    }

    /// Resolve "provider:model" spec → Arc<dyn LlmProvider>.
    /// Lazy-creates and caches provider instances.
    pub fn resolve(&self, spec: &str) -> anyhow::Result<Arc<dyn LlmProvider>> {
        let (provider_name, model_ref) = self.parse_spec(spec)?;

        let config = self.configs.get(&provider_name)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: '{}'", provider_name))?;

        // Resolve alias → raw model ID
        let (resolved_model_id, model_entry) = if let Some(entry) = config.models.get(model_ref) {
            (entry.id.clone(), Some(entry))
        } else {
            (model_ref.to_string(), None)
        };

        let cache_key = format!("{}:{}", provider_name, resolved_model_id);

        // Fast path: read lock
        {
            let cache = self.cache.read().unwrap();
            if let Some(provider) = cache.get(&cache_key) {
                return Ok(provider.clone());
            }
        }

        // Slow path: write lock with double-check
        let mut cache = self.cache.write().unwrap();
        if let Some(provider) = cache.get(&cache_key) {
            return Ok(provider.clone());
        }

        let provider = self.create_provider(config, &resolved_model_id, model_entry)?;
        let provider: Arc<dyn LlmProvider> = Arc::new(provider);
        cache.insert(cache_key, provider.clone());
        Ok(provider)
    }

    /// Parse spec into (provider_name, model_ref).
    fn parse_spec<'a>(&'a self, spec: &'a str) -> anyhow::Result<(String, &'a str)> {
        if let Some((left, right)) = spec.split_once(':') {
            if self.configs.contains_key(left) {
                return Ok((left.to_string(), right));
            }
            // Left side is not a known provider — treat whole string as model
        }
        // No colon or unknown provider prefix → use default provider
        Ok((self.default_provider.clone(), spec))
    }

    fn create_provider(
        &self,
        config: &ProviderConfig,
        model_id: &str,
        model_entry: Option<&ModelEntry>,
    ) -> anyhow::Result<ChatCompletionsProvider> {
        let api_base = config.api_base.as_deref()
            .ok_or_else(|| anyhow::anyhow!(
                "api_base required for provider (type: {})", config.provider_type
            ))?;

        let provider = ChatCompletionsProvider::new(
            api_base.to_string(),
            config.api_key.clone(),
            model_id.to_string(),
        );

        // Apply ModelInfo overrides if present
        if let Some(entry) = model_entry {
            if entry.context_window.is_some() || entry.max_output_tokens.is_some() {
                let info = ModelInfo {
                    id: model_id.to_string(),
                    name: model_id.to_string(),
                    context_window: entry.context_window.unwrap_or(128_000),
                    max_output_tokens: entry.max_output_tokens.unwrap_or(4096),
                    capabilities: Capabilities {
                        tool_use: true,
                        vision: true,
                        streaming: true,
                        extended_thinking: false,
                    },
                };
                return Ok(provider.with_model_info(info));
            }
        }

        Ok(provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ProviderConfig {
        let mut models = HashMap::new();
        models.insert("sonnet".into(), ModelEntry {
            id: "anthropic/claude-sonnet-4-20250514".into(),
            context_window: None,
            max_output_tokens: None,
        });
        models.insert("flash".into(), ModelEntry {
            id: "google/gemini-2.5-flash".into(),
            context_window: Some(1_000_000),
            max_output_tokens: Some(65536),
        });
        ProviderConfig {
            provider_type: "openai".into(),
            api_base: Some("https://openrouter.ai/api/v1".into()),
            api_key: Some("sk-test".into()),
            models,
        }
    }

    #[test]
    fn test_resolve_alias() {
        let mut registry = ProviderRegistry::new("or".into(), Some("sonnet".into()));
        registry.register("or".into(), test_config());
        let provider = registry.resolve("or:sonnet").unwrap();
        assert_eq!(provider.model_info().id, "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn test_resolve_raw_model_id() {
        let mut registry = ProviderRegistry::new("or".into(), Some("sonnet".into()));
        registry.register("or".into(), test_config());
        let provider = registry.resolve("or:anthropic/claude-haiku").unwrap();
        assert_eq!(provider.model_info().id, "anthropic/claude-haiku");
    }

    #[test]
    fn test_resolve_default_provider() {
        let mut registry = ProviderRegistry::new("or".into(), Some("sonnet".into()));
        registry.register("or".into(), test_config());
        let provider = registry.resolve("sonnet").unwrap();
        assert_eq!(provider.model_info().id, "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn test_resolve_colon_in_model_id_fallback() {
        let mut registry = ProviderRegistry::new("or".into(), Some("sonnet".into()));
        registry.register("or".into(), test_config());
        let provider = registry.resolve("google/gemma-3-27b-it:free").unwrap();
        assert_eq!(provider.model_info().id, "google/gemma-3-27b-it:free");
    }

    #[test]
    fn test_resolve_unknown_provider() {
        let registry = ProviderRegistry::new("or".into(), None);
        let err = registry.resolve("nonexistent:model")
            .err().expect("should fail");
        assert!(err.to_string().contains("Unknown provider"));
    }

    #[test]
    fn test_resolve_caches_same_instance() {
        let mut registry = ProviderRegistry::new("or".into(), Some("sonnet".into()));
        registry.register("or".into(), test_config());
        let p1 = registry.resolve("or:sonnet").unwrap();
        let p2 = registry.resolve("or:sonnet").unwrap();
        assert!(Arc::ptr_eq(&p1, &p2));
    }

    #[test]
    fn test_resolve_model_info_overrides() {
        let mut registry = ProviderRegistry::new("or".into(), Some("flash".into()));
        registry.register("or".into(), test_config());
        let provider = registry.resolve("or:flash").unwrap();
        assert_eq!(provider.model_info().context_window, 1_000_000);
        assert_eq!(provider.model_info().max_output_tokens, 65536);
    }

    #[test]
    fn test_default_provider() {
        let mut registry = ProviderRegistry::new("or".into(), Some("sonnet".into()));
        registry.register("or".into(), test_config());
        let provider = registry.default_provider().unwrap();
        assert_eq!(provider.model_info().id, "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn test_default_provider_no_default_model() {
        let mut registry = ProviderRegistry::new("or".into(), None);
        registry.register("or".into(), test_config());
        let err = registry.default_provider()
            .err().expect("should fail");
        assert!(err.to_string().contains("No default model"));
    }

    #[test]
    fn test_resolve_missing_api_base() {
        let mut registry = ProviderRegistry::new("bad".into(), Some("m".into()));
        registry.register("bad".into(), ProviderConfig {
            provider_type: "openai".into(),
            api_base: None,
            api_key: None,
            models: HashMap::new(),
        });
        let err = registry.resolve("bad:m")
            .err().expect("should fail");
        assert!(err.to_string().contains("api_base required"));
    }

    #[test]
    fn test_set_default_changes_provider() {
        let mut registry = ProviderRegistry::new("or".into(), Some("sonnet".into()));
        registry.register("or".into(), test_config());

        let mut vllm_config = ProviderConfig {
            provider_type: "openai".into(),
            api_base: Some("http://localhost:8000/v1".into()),
            api_key: None,
            models: HashMap::new(),
        };
        vllm_config.models.insert("qwen".into(), ModelEntry {
            id: "Qwen/Qwen2.5-72B".into(),
            context_window: None,
            max_output_tokens: None,
        });
        registry.register("vllm".into(), vllm_config);
        registry.set_default("vllm".into(), Some("qwen".into()));

        let provider = registry.default_provider().unwrap();
        assert_eq!(provider.model_info().id, "Qwen/Qwen2.5-72B");
    }

    #[test]
    fn test_cli_provider_registration() {
        let mut registry = ProviderRegistry::new("_cli".into(), Some("gpt-4o".into()));
        registry.register("_cli".into(), ProviderConfig {
            provider_type: "openai".into(),
            api_base: Some("https://api.openai.com/v1".into()),
            api_key: Some("sk-test".into()),
            models: HashMap::new(),
        });
        let provider = registry.resolve("gpt-4o").unwrap();
        assert_eq!(provider.model_info().id, "gpt-4o");
    }
}
