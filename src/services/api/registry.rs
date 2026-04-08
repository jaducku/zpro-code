use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::openai_provider::ChatCompletionsProvider;
use super::provider::LlmProvider;
use super::types::{Capabilities, ModelInfo};

/// Connection template for a provider (api_base + optional api_key).
#[derive(Debug, Clone)]
pub struct ProviderTemplate {
    pub api_base: String,
    pub api_key: Option<String>,
}

/// A fully resolved LLM entry: connection info + model ID + optional overrides.
#[derive(Debug, Clone)]
pub struct ResolvedLlm {
    pub api_base: String,
    pub api_key: Option<String>,
    pub model_id: String,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u32>,
    /// Enable reasoning/thinking extraction at the provider level.
    pub reasoning: Option<bool>,
    /// Reasoning effort level (e.g., "high", "medium", "low").
    pub reasoning_effort: Option<String>,
}

/// Model-centric provider registry with lazy creation and caching.
pub struct ProviderRegistry {
    provider_templates: HashMap<String, ProviderTemplate>,
    llm_configs: HashMap<String, ResolvedLlm>,
    /// Legacy model aliases from AtomConfig.models (alias → raw model ID).
    /// Resolved via _cli fallback at resolve() time.
    legacy_models: HashMap<String, String>,
    cache: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
    default_model: Option<String>,
}

impl ProviderRegistry {
    pub fn new(default_model: Option<String>) -> Self {
        Self {
            provider_templates: HashMap::new(),
            llm_configs: HashMap::new(),
            legacy_models: HashMap::new(),
            cache: RwLock::new(HashMap::new()),
            default_model,
        }
    }

    /// Build from AtomConfig (v4 format: array-based providers).
    ///
    /// Config format:
    /// - `providers`: array of provider configs with embedded models
    /// - Each model has alias, id, and optional constraints/params
    pub fn from_config(config: &crate::services::config::AtomConfig) -> anyhow::Result<Self> {
        let mut registry = Self::new(config.default_model.clone());

        // Load providers and their models
        for provider in &config.providers {
            if provider.name.starts_with('_') {
                anyhow::bail!(
                    "Provider name '{}' is reserved (starts with '_')",
                    provider.name
                );
            }

            // Register provider template
            registry.provider_templates.insert(
                provider.name.clone(),
                ProviderTemplate {
                    api_base: provider.auth.api_base.clone(),
                    api_key: provider.auth.api_key.clone(),
                },
            );

            // Register models
            for model in &provider.models {
                if model.alias.starts_with('_') {
                    anyhow::bail!(
                        "Model alias '{}' is reserved (starts with '_')",
                        model.alias
                    );
                }

                let (reasoning, reasoning_effort) = model
                    .params
                    .as_ref()
                    .and_then(|p| p.reasoning.as_ref())
                    .map(|r| (Some(r.enabled), r.effort.clone()))
                    .unwrap_or((None, None));

                registry.llm_configs.insert(
                    model.alias.clone(),
                    ResolvedLlm {
                        api_base: provider.auth.api_base.clone(),
                        api_key: provider.auth.api_key.clone(),
                        model_id: model.id.clone(),
                        context_window: model.constraints.as_ref().and_then(|c| c.context_window),
                        max_output_tokens: model
                            .constraints
                            .as_ref()
                            .and_then(|c| c.max_output_tokens),
                        reasoning,
                        reasoning_effort,
                    },
                );
            }
        }

        Ok(registry)
    }

    /// Register a provider template (for CLI or programmatic use).
    pub fn register_provider(&mut self, name: String, api_base: String, api_key: Option<String>) {
        self.provider_templates
            .insert(name, ProviderTemplate { api_base, api_key });
    }

    /// Register an LLM entry (for CLI or programmatic use).
    pub fn register_llm(&mut self, name: String, llm: ResolvedLlm) {
        self.llm_configs.insert(name, llm);
    }

    /// Set the default model name.
    pub fn set_default_model(&mut self, model: Option<String>) {
        if model.is_some() {
            self.default_model = model;
        }
    }

    /// Get the default provider+model.
    pub fn default_provider(&self) -> anyhow::Result<Arc<dyn LlmProvider>> {
        let model = self
            .default_model
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No default model configured"))?;
        self.resolve(model)
    }

    /// Resolve a model spec → Arc<dyn LlmProvider>.
    ///
    /// Resolution order:
    /// 1. Named LLM in `llm_configs`
    /// 2. Ad-hoc `provider:raw_model_id` via `provider_templates`
    /// 3. Fallback to `_cli` template with spec as raw model ID
    /// 4. Legacy model alias (from AtomConfig.models) via `_cli` template
    pub fn resolve(&self, spec: &str) -> anyhow::Result<Arc<dyn LlmProvider>> {
        // 1. Named LLM
        if let Some(llm) = self.llm_configs.get(spec) {
            return self.resolve_cached(spec, llm);
        }

        // 2. Ad-hoc provider:model
        if let Some((left, right)) = spec.split_once(':') {
            if let Some(tmpl) = self.provider_templates.get(left) {
                let llm = ResolvedLlm {
                    api_base: tmpl.api_base.clone(),
                    api_key: tmpl.api_key.clone(),
                    model_id: right.to_string(),
                    context_window: None,
                    max_output_tokens: None,
                    reasoning: None,
                    reasoning_effort: None,
                };
                let cache_key = format!("{}:{}", left, right);
                return self.resolve_cached(&cache_key, &llm);
            }
            // Left side is not a known provider — check if the whole spec
            // is a known provider template before falling through
            if self.provider_templates.contains_key(left) {
                // Shouldn't reach here since we handled it above, but just in case
            } else if !self.provider_templates.contains_key("_cli") {
                // Has a colon, left side unknown, no _cli fallback
                return Err(anyhow::anyhow!("Unknown provider: '{}'", left));
            }
        }

        // 3. Fallback to _cli (with legacy alias expansion)
        if let Some(tmpl) = self.provider_templates.get("_cli") {
            // Expand legacy model alias if present, otherwise use spec as-is
            let model_id = self
                .legacy_models
                .get(spec)
                .cloned()
                .unwrap_or_else(|| spec.to_string());
            let llm = ResolvedLlm {
                api_base: tmpl.api_base.clone(),
                api_key: tmpl.api_key.clone(),
                model_id,
                context_window: None,
                max_output_tokens: None,
                reasoning: None,
                reasoning_effort: None,
            };
            let cache_key = format!("_cli:{}", spec);
            return self.resolve_cached(&cache_key, &llm);
        }

        Err(anyhow::anyhow!(
            "Unknown model '{}': not a named LLM, not a provider:model spec, and no _cli fallback configured",
            spec
        ))
    }

    /// Resolve from cache or create + cache a provider for the given LLM.
    fn resolve_cached(
        &self,
        cache_key: &str,
        llm: &ResolvedLlm,
    ) -> anyhow::Result<Arc<dyn LlmProvider>> {
        // Fast path: read lock
        {
            let cache = self.cache.read().unwrap();
            if let Some(provider) = cache.get(cache_key) {
                return Ok(provider.clone());
            }
        }

        // Slow path: write lock with double-check
        let mut cache = self.cache.write().unwrap();
        if let Some(provider) = cache.get(cache_key) {
            return Ok(provider.clone());
        }

        let provider = self.create_provider(llm)?;
        let provider: Arc<dyn LlmProvider> = Arc::new(provider);
        cache.insert(cache_key.to_string(), provider.clone());
        Ok(provider)
    }

    fn create_provider(&self, llm: &ResolvedLlm) -> anyhow::Result<ChatCompletionsProvider> {
        let mut provider = ChatCompletionsProvider::new(
            llm.api_base.clone(),
            llm.api_key.clone(),
            llm.model_id.clone(),
        );

        // Apply reasoning config if enabled for this model
        if llm.reasoning == Some(true) {
            provider = provider.with_reasoning(llm.reasoning_effort.clone());
        }

        if llm.context_window.is_some() || llm.max_output_tokens.is_some() {
            let info = ModelInfo {
                id: llm.model_id.clone(),
                name: llm.model_id.clone(),
                context_window: llm.context_window.unwrap_or(128_000),
                max_output_tokens: llm.max_output_tokens.unwrap_or(4096),
                capabilities: Capabilities {
                    tool_use: true,
                    vision: true,
                    streaming: true,
                    extended_thinking: false,
                },
            };
            return Ok(provider.with_model_info(info));
        }

        Ok(provider)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_registry() -> ProviderRegistry {
        let mut r = ProviderRegistry::new(None);
        r.register_provider(
            "or".into(),
            "https://openrouter.ai/api/v1".into(),
            Some("sk-test".into()),
        );
        r.register_provider("ollama".into(), "http://localhost:11434/v1".into(), None);
        r.register_llm(
            "sonnet".into(),
            ResolvedLlm {
                api_base: "https://openrouter.ai/api/v1".into(),
                api_key: Some("sk-test".into()),
                model_id: "anthropic/claude-sonnet-4-20250514".into(),
                context_window: None,
                max_output_tokens: None,
                reasoning: None,
                reasoning_effort: None,
            },
        );
        r.register_llm(
            "flash".into(),
            ResolvedLlm {
                api_base: "https://openrouter.ai/api/v1".into(),
                api_key: Some("sk-test".into()),
                model_id: "google/gemini-2.5-flash".into(),
                context_window: Some(1_000_000),
                max_output_tokens: Some(65536),
                reasoning: None,
                reasoning_effort: None,
            },
        );
        r.register_llm(
            "llama".into(),
            ResolvedLlm {
                api_base: "http://localhost:11434/v1".into(),
                api_key: None,
                model_id: "llama3.2".into(),
                context_window: None,
                max_output_tokens: None,
                reasoning: None,
                reasoning_effort: None,
            },
        );
        r.set_default_model(Some("sonnet".into()));
        r
    }

    #[test]
    fn test_resolve_named_llm() {
        let r = build_test_registry();
        let p = r.resolve("sonnet").unwrap();
        assert_eq!(p.model_info().id, "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn test_resolve_ad_hoc_provider_model() {
        let r = build_test_registry();
        let p = r.resolve("or:anthropic/claude-haiku").unwrap();
        assert_eq!(p.model_info().id, "anthropic/claude-haiku");
    }

    #[test]
    fn test_resolve_model_info_overrides() {
        let r = build_test_registry();
        let p = r.resolve("flash").unwrap();
        assert_eq!(p.model_info().context_window, 1_000_000);
        assert_eq!(p.model_info().max_output_tokens, 65536);
    }

    #[test]
    fn test_resolve_caches() {
        let r = build_test_registry();
        let p1 = r.resolve("sonnet").unwrap();
        let p2 = r.resolve("sonnet").unwrap();
        assert!(Arc::ptr_eq(&p1, &p2));
    }

    #[test]
    fn test_resolve_unknown_model_no_cli() {
        let r = build_test_registry();
        let err = r.resolve("nonexistent").err().expect("should fail");
        assert!(err.to_string().contains("Unknown model"));
    }

    #[test]
    fn test_resolve_unknown_model_with_cli_fallback() {
        let mut r = build_test_registry();
        r.register_provider(
            "_cli".into(),
            "https://api.openai.com/v1".into(),
            Some("sk-cli".into()),
        );
        let p = r.resolve("gpt-4o").unwrap();
        assert_eq!(p.model_info().id, "gpt-4o");
    }

    #[test]
    fn test_resolve_unknown_provider_in_adhoc() {
        let r = build_test_registry();
        let err = r.resolve("unknown:model").err().expect("should fail");
        assert!(err.to_string().contains("Unknown provider"));
    }

    #[test]
    fn test_default_provider() {
        let r = build_test_registry();
        let p = r.default_provider().unwrap();
        assert_eq!(p.model_info().id, "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn test_default_provider_no_model() {
        let r = ProviderRegistry::new(None);
        let err = r.default_provider().err().expect("should fail");
        assert!(err.to_string().contains("No default model"));
    }

    #[test]
    fn test_from_config_v4() {
        let json = r#"{
            "default_model": "sonnet",
            "providers": [
                {
                    "name": "or",
                    "auth": {
                        "api_base": "https://openrouter.ai/api/v1",
                        "api_key": "sk-test"
                    },
                    "models": [
                        {
                            "alias": "sonnet",
                            "id": "anthropic/claude-sonnet-4-20250514",
                            "capabilities": [],
                            "constraints": { "context_window": 200000 }
                        }
                    ]
                },
                {
                    "name": "local",
                    "auth": {
                        "api_base": "http://localhost:8080/v1"
                    },
                    "models": [
                        {
                            "alias": "qwen",
                            "id": "Qwen/Qwen2.5-72B",
                            "capabilities": []
                        }
                    ]
                }
            ]
        }"#;
        let config: crate::services::config::AtomConfig = serde_json::from_str(json).unwrap();
        let r = ProviderRegistry::from_config(&config).unwrap();
        let p = r.resolve("sonnet").unwrap();
        assert_eq!(p.model_info().id, "anthropic/claude-sonnet-4-20250514");
        let p2 = r.resolve("qwen").unwrap();
        assert_eq!(p2.model_info().id, "Qwen/Qwen2.5-72B");
    }

    #[test]
    fn test_from_config_reserved_name_provider() {
        let json = r#"{
            "providers": [
                {
                    "name": "_bad",
                    "auth": { "api_base": "http://x" },
                    "models": []
                }
            ]
        }"#;
        let config: crate::services::config::AtomConfig = serde_json::from_str(json).unwrap();
        let err = ProviderRegistry::from_config(&config)
            .err()
            .expect("should fail");
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn test_from_config_reserved_name_model() {
        let json = r#"{
            "providers": [
                {
                    "name": "p",
                    "auth": { "api_base": "http://x" },
                    "models": [
                        {
                            "alias": "_bad",
                            "id": "model",
                            "capabilities": []
                        }
                    ]
                }
            ]
        }"#;
        let config: crate::services::config::AtomConfig = serde_json::from_str(json).unwrap();
        let err = ProviderRegistry::from_config(&config)
            .err()
            .expect("should fail");
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn test_from_config_ad_hoc_resolve_still_works() {
        let json = r#"{
            "providers": [
                {
                    "name": "or",
                    "auth": {
                        "api_base": "https://openrouter.ai/api/v1",
                        "api_key": "sk-test"
                    },
                    "models": []
                }
            ]
        }"#;
        let config: crate::services::config::AtomConfig = serde_json::from_str(json).unwrap();
        let r = ProviderRegistry::from_config(&config).unwrap();
        // Ad-hoc provider:model still works even without a models entry
        let p = r.resolve("or:anthropic/claude-haiku").unwrap();
        assert_eq!(p.model_info().id, "anthropic/claude-haiku");
    }
}
