use serde::{Deserialize, Serialize};
use std::path::Path;

/// Provider authentication credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAuth {
    pub api_base: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

/// Model-level constraints (context window, max output tokens).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelConstraints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
}

/// Reasoning configuration for extended thinking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningParams {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>, // "high" | "medium" | "low"
}

/// Model-level parameters (temperature, top_p, reasoning, etc).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningParams>,
}

/// Pricing information for a model.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Pricing {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_per_mtok: Option<f64>, // $ per million input tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_per_mtok: Option<f64>, // $ per million output tokens
}

/// A single model configuration within a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Alias used to reference this model (e.g., "sonnet", "fast").
    pub alias: String,
    /// Actual model ID (e.g., "anthropic/claude-sonnet-4-20250514").
    pub id: String,
    /// Model capabilities (e.g., ["reasoning", "code", "vision"]).
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Model constraints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<ModelConstraints>,
    /// Model parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<ModelParams>,
    /// Pricing information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing: Option<Pricing>,
    /// Fallback model aliases in priority order.
    #[serde(default)]
    pub fallback: Vec<String>,
}

/// A provider configuration with models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub auth: ProviderAuth,
    pub models: Vec<ModelConfig>,
}

/// A preset configuration for common use cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub params: ModelParams,
}

/// Global Atom Code configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AtomConfig {
    /// Default model alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,

    /// Provider registry (v4 format: array-based).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<ProviderConfig>,

    /// Preset configurations.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<Preset>,

    /// --- Legacy fields (backward compat) ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
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

    /// Create a sample config with OpenRouter pre-registered.
    pub fn save_sample() -> anyhow::Result<()> {
        let path = crate::utils::paths::global_config_dir().join("config.json");
        Self::save_sample_to(&path)
    }

    pub fn save_sample_to(path: &Path) -> anyhow::Result<()> {
        let providers = vec![ProviderConfig {
            name: "openrouter".to_string(),
            auth: ProviderAuth {
                api_base: "https://openrouter.ai/api/v1".to_string(),
                api_key: Some("YOUR_OPENROUTER_API_KEY".to_string()),
            },
            models: vec![
                ModelConfig {
                    alias: "sonnet".to_string(),
                    id: "anthropic/claude-sonnet-4-20250514".to_string(),
                    capabilities: vec![
                        "reasoning".to_string(),
                        "code".to_string(),
                        "vision".to_string(),
                    ],
                    constraints: Some(ModelConstraints {
                        context_window: Some(200_000),
                        max_output_tokens: Some(16_000),
                    }),
                    params: Some(ModelParams {
                        reasoning: Some(ReasoningParams {
                            enabled: true,
                            effort: Some("high".to_string()),
                        }),
                        ..Default::default()
                    }),
                    pricing: Some(Pricing {
                        input_per_mtok: Some(3.0),
                        output_per_mtok: Some(15.0),
                    }),
                    fallback: vec!["free".to_string()],
                },
                ModelConfig {
                    alias: "flash".to_string(),
                    id: "google/gemini-2.5-flash-preview-04-17".to_string(),
                    capabilities: vec!["code".to_string(), "vision".to_string()],
                    constraints: Some(ModelConstraints {
                        context_window: Some(1_000_000),
                        max_output_tokens: Some(65_536),
                    }),
                    params: None,
                    pricing: Some(Pricing {
                        input_per_mtok: Some(0.15),
                        output_per_mtok: Some(0.60),
                    }),
                    fallback: vec![],
                },
                ModelConfig {
                    alias: "free".to_string(),
                    id: "google/gemma-3-27b-it:free".to_string(),
                    capabilities: vec!["code".to_string()],
                    constraints: Some(ModelConstraints {
                        context_window: Some(8_192),
                        max_output_tokens: None,
                    }),
                    params: None,
                    pricing: None,
                    fallback: vec![],
                },
            ],
        }];

        let presets = vec![
            Preset {
                name: "code".to_string(),
                description: Some("Code generation and implementation".to_string()),
                params: ModelParams {
                    temperature: Some(0.5),
                    top_p: Some(0.9),
                    ..Default::default()
                },
            },
            Preset {
                name: "reasoning".to_string(),
                description: Some("Complex reasoning tasks with extended thinking".to_string()),
                params: ModelParams {
                    temperature: Some(1.0),
                    reasoning: Some(ReasoningParams {
                        enabled: true,
                        effort: Some("high".to_string()),
                    }),
                    ..Default::default()
                },
            },
            Preset {
                name: "creative".to_string(),
                description: Some("Creative writing and brainstorming".to_string()),
                params: ModelParams {
                    temperature: Some(1.2),
                    top_p: Some(1.0),
                    ..Default::default()
                },
            },
        ];

        let config = AtomConfig {
            default_model: Some("free".to_string()),
            providers,
            presets,
            ..Default::default()
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
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

    /// Find a model by alias, returning the provider and model config.
    pub fn find_model(&self, alias: &str) -> Option<(&ProviderConfig, &ModelConfig)> {
        self.providers
            .iter()
            .find_map(|p| p.models.iter().find(|m| m.alias == alias).map(|m| (p, m)))
    }

    /// Resolve a model alias to "provider:model_id" string.
    pub fn resolve_model(&self, alias: &str) -> Option<String> {
        self.find_model(alias)
            .map(|(provider, model)| format!("{}:{}", provider.name, model.id))
    }

    /// List all registered models as (alias, "provider:id").
    pub fn list_models(&self) -> Vec<(String, String)> {
        let mut models: Vec<_> = self
            .providers
            .iter()
            .flat_map(|p| {
                p.models
                    .iter()
                    .map(|m| (m.alias.clone(), format!("{}:{}", p.name, m.id)))
            })
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
        assert!(config.providers.is_empty());
        assert!(config.presets.is_empty());
        assert!(config.default_model.is_none());
    }

    #[test]
    fn test_find_model() {
        let mut config = AtomConfig::default();
        config.providers.push(ProviderConfig {
            name: "test".to_string(),
            auth: ProviderAuth {
                api_base: "http://localhost:8000/v1".to_string(),
                api_key: None,
            },
            models: vec![ModelConfig {
                alias: "fast".to_string(),
                id: "qwen-fast".to_string(),
                capabilities: vec![],
                constraints: None,
                params: None,
                pricing: None,
                fallback: vec![],
            }],
        });

        let result = config.find_model("fast");
        assert!(result.is_some());
        let (provider, model) = result.unwrap();
        assert_eq!(provider.name, "test");
        assert_eq!(model.id, "qwen-fast");
    }

    #[test]
    fn test_resolve_model() {
        let mut config = AtomConfig::default();
        config.providers.push(ProviderConfig {
            name: "openrouter".to_string(),
            auth: ProviderAuth {
                api_base: "https://openrouter.ai/api/v1".to_string(),
                api_key: Some("sk-test".to_string()),
            },
            models: vec![ModelConfig {
                alias: "sonnet".to_string(),
                id: "anthropic/claude-sonnet-4-20250514".to_string(),
                capabilities: vec!["reasoning".to_string()],
                constraints: None,
                params: None,
                pricing: None,
                fallback: vec![],
            }],
        });

        assert_eq!(
            config.resolve_model("sonnet"),
            Some("openrouter:anthropic/claude-sonnet-4-20250514".to_string())
        );
        assert_eq!(config.resolve_model("nonexistent"), None);
    }

    #[test]
    fn test_list_models() {
        let mut config = AtomConfig::default();
        config.providers.push(ProviderConfig {
            name: "or".to_string(),
            auth: ProviderAuth {
                api_base: "https://openrouter.ai/api/v1".to_string(),
                api_key: None,
            },
            models: vec![
                ModelConfig {
                    alias: "sonnet".to_string(),
                    id: "anthropic/claude-sonnet-4-20250514".to_string(),
                    capabilities: vec![],
                    constraints: None,
                    params: None,
                    pricing: None,
                    fallback: vec![],
                },
                ModelConfig {
                    alias: "haiku".to_string(),
                    id: "anthropic/claude-haiku".to_string(),
                    capabilities: vec![],
                    constraints: None,
                    params: None,
                    pricing: None,
                    fallback: vec![],
                },
            ],
        });

        let models = config.list_models();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].0, "haiku"); // sorted alphabetically
        assert_eq!(models[1].0, "sonnet");
    }

    #[test]
    fn test_save_sample_creates_valid_config() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.json");
        AtomConfig::save_sample_to(&path).unwrap();

        assert!(path.exists());
        let config = AtomConfig::load_from(&path);

        assert_eq!(config.default_model.as_deref(), Some("free"));
        assert_eq!(config.providers.len(), 1);

        let or_provider = &config.providers[0];
        assert_eq!(or_provider.name, "openrouter");
        assert_eq!(or_provider.models.len(), 3);

        // Check sonnet model has reasoning enabled
        let sonnet = or_provider
            .models
            .iter()
            .find(|m| m.alias == "sonnet")
            .unwrap();
        assert!(sonnet.capabilities.contains(&"reasoning".to_string()));
        assert!(sonnet.constraints.is_some());
        assert_eq!(
            sonnet.constraints.as_ref().unwrap().context_window,
            Some(200_000)
        );

        // Check presets
        assert_eq!(config.presets.len(), 3);
        let code_preset = config.presets.iter().find(|p| p.name == "code").unwrap();
        assert_eq!(code_preset.params.temperature, Some(0.5));
    }

    #[test]
    fn test_parse_v4_config() {
        let json = r#"{
            "default_model": "sonnet",
            "providers": [
                {
                    "name": "openrouter",
                    "auth": {
                        "api_base": "https://openrouter.ai/api/v1",
                        "api_key": "sk-test"
                    },
                    "models": [
                        {
                            "alias": "sonnet",
                            "id": "anthropic/claude-sonnet-4-20250514",
                            "capabilities": ["reasoning", "code"],
                            "constraints": {
                                "context_window": 200000
                            },
                            "params": {
                                "reasoning": {
                                    "enabled": true,
                                    "effort": "high"
                                }
                            }
                        }
                    ]
                }
            ],
            "presets": [
                {
                    "name": "reasoning",
                    "description": "Complex tasks",
                    "params": {
                        "temperature": 1.0,
                        "reasoning": {
                            "enabled": true,
                            "effort": "high"
                        }
                    }
                }
            ]
        }"#;

        let config: AtomConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.default_model.as_deref(), Some("sonnet"));
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.presets.len(), 1);

        let provider = &config.providers[0];
        assert_eq!(provider.name, "openrouter");
        assert_eq!(provider.models.len(), 1);

        let model = &provider.models[0];
        assert_eq!(model.alias, "sonnet");
        assert!(
            model
                .params
                .as_ref()
                .unwrap()
                .reasoning
                .as_ref()
                .unwrap()
                .enabled
        );

        let preset = &config.presets[0];
        assert_eq!(preset.name, "reasoning");
        assert_eq!(preset.params.temperature, Some(1.0));
    }

    #[test]
    fn test_roundtrip_v4_config() {
        let config = AtomConfig {
            default_model: Some("sonnet".to_string()),
            providers: vec![ProviderConfig {
                name: "test".to_string(),
                auth: ProviderAuth {
                    api_base: "http://test".to_string(),
                    api_key: Some("key".to_string()),
                },
                models: vec![ModelConfig {
                    alias: "m1".to_string(),
                    id: "model-1".to_string(),
                    capabilities: vec!["code".to_string()],
                    constraints: Some(ModelConstraints {
                        context_window: Some(8192),
                        max_output_tokens: Some(4096),
                    }),
                    params: Some(ModelParams {
                        temperature: Some(0.7),
                        ..Default::default()
                    }),
                    pricing: None,
                    fallback: vec![],
                }],
            }],
            presets: vec![Preset {
                name: "p1".to_string(),
                description: Some("Preset 1".to_string()),
                params: ModelParams {
                    temperature: Some(0.5),
                    ..Default::default()
                },
            }],
            ..Default::default()
        };

        let json = serde_json::to_string_pretty(&config).unwrap();
        let parsed: AtomConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.default_model, config.default_model);
        assert_eq!(parsed.providers.len(), 1);
        assert_eq!(parsed.presets.len(), 1);
    }
}
