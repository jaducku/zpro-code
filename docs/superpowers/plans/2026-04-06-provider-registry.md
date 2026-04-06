# Provider Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a multi-provider registry to config.json with lazy creation, colon-separated reference syntax (`provider:model`), and injection through QueryContext/ToolUseContext/HookContext.

**Architecture:** New `ProviderRegistry` struct in `src/services/api/registry.rs` loaded from `AtomConfig.providers`. Lazy provider creation cached by `provider:resolved_model_id`. Registry injected as `Arc<ProviderRegistry>` through existing context types. CLI args register as `_cli` provider for backward compatibility.

**Tech Stack:** Rust, serde (untagged enum for ModelEntry), std::sync::RwLock, Arc

**Spec:** `docs/superpowers/specs/2026-04-06-provider-registry-design.md`

---

### Task 1: Config types — ModelEntryJson, ProviderConfigJson, AtomConfig extensions

**Files:**
- Modify: `src/services/config/mod.rs`

- [ ] **Step 1: Write failing tests for new config deserialization**

```rust
#[test]
fn test_parse_config_with_providers() {
    let json = r#"{
        "providers": {
            "or": {
                "type": "openai",
                "api_base": "https://openrouter.ai/api/v1",
                "api_key": "sk-or-test",
                "models": {
                    "sonnet": "anthropic/claude-sonnet-4-20250514",
                    "flash": {
                        "id": "google/gemini-2.5-flash",
                        "context_window": 1000000,
                        "max_output_tokens": 65536
                    }
                }
            },
            "vllm": {
                "type": "openai",
                "api_base": "http://localhost:8000/v1",
                "models": {
                    "qwen": { "id": "Qwen/Qwen2.5-72B" }
                }
            }
        },
        "default_provider": "or",
        "default_model": "sonnet"
    }"#;
    let config: AtomConfig = serde_json::from_str(json).unwrap();
    let providers = config.providers.as_ref().unwrap();
    assert_eq!(providers.len(), 2);

    let or = &providers["or"];
    assert_eq!(or.api_base.as_deref(), Some("https://openrouter.ai/api/v1"));
    let models = or.models.as_ref().unwrap();
    assert_eq!(models.len(), 2);

    // String shorthand
    match &models["sonnet"] {
        ModelEntryJson::Short(id) => assert_eq!(id, "anthropic/claude-sonnet-4-20250514"),
        _ => panic!("Expected Short variant"),
    }
    // Object form
    match &models["flash"] {
        ModelEntryJson::Full { id, context_window, max_output_tokens } => {
            assert_eq!(id, "google/gemini-2.5-flash");
            assert_eq!(*context_window, Some(1_000_000));
            assert_eq!(*max_output_tokens, Some(65536));
        }
        _ => panic!("Expected Full variant"),
    }

    assert_eq!(config.default_provider.as_deref(), Some("or"));
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
    assert_eq!(config.provider.as_deref(), Some("openrouter"));
    assert_eq!(config.default_model(), Some("google/gemma-3-27b-it:free".into()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib config::tests::test_parse_config_with_providers -- --nocapture`
Expected: FAIL — `providers` field not recognized

- [ ] **Step 3: Add types and fields to AtomConfig**

Add to `src/services/config/mod.rs`:

```rust
/// Deserializes from either:
/// - String: "anthropic/claude-sonnet-4-20250514" (shorthand)
/// - Object: { "id": "...", "context_window": 200000 }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelEntryJson {
    Short(String),
    Full {
        id: String,
        context_window: Option<u64>,
        max_output_tokens: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfigJson {
    #[serde(rename = "type")]
    pub provider_type: Option<String>,
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub models: Option<HashMap<String, ModelEntryJson>>,
}
```

Add to `AtomConfig` struct:

```rust
#[serde(default)]
pub providers: Option<HashMap<String, ProviderConfigJson>>,

#[serde(default)]
pub default_provider: Option<String>,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib config::tests -- --nocapture`
Expected: All config tests pass

- [ ] **Step 5: Commit**

```bash
git add src/services/config/mod.rs
git commit -m "feat(config): add providers, ModelEntryJson, ProviderConfigJson types"
```

---

### Task 2: ProviderRegistry core — struct, from_config, resolve, register

**Files:**
- Create: `src/services/api/registry.rs`
- Modify: `src/services/api/mod.rs` (add `pub mod registry;`)

- [ ] **Step 1: Write failing tests for registry resolve**

```rust
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

        // "google/gemma-3-27b-it:free" — left of colon is not a known provider
        let provider = registry.resolve("google/gemma-3-27b-it:free").unwrap();
        assert_eq!(provider.model_info().id, "google/gemma-3-27b-it:free");
    }

    #[test]
    fn test_resolve_unknown_provider() {
        let registry = ProviderRegistry::new("or".into(), None);
        let err = registry.resolve("nonexistent:model").unwrap_err();
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

        // No default model — should error
        let err = registry.default_provider().unwrap_err();
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
        let err = registry.resolve("bad:m").unwrap_err();
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

        // _cli with raw model ID (no alias)
        let provider = registry.resolve("gpt-4o").unwrap();
        assert_eq!(provider.model_info().id, "gpt-4o");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib api::registry::tests -- --nocapture`
Expected: FAIL — module doesn't exist

- [ ] **Step 3: Implement ProviderRegistry**

Create `src/services/api/registry.rs`:

```rust
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

        // Slow path: write lock with entry API
        let mut cache = self.cache.write().unwrap();
        // Double-check after acquiring write lock
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
```

Add to `src/services/api/mod.rs`:

```rust
pub mod registry;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib api::registry::tests -- --nocapture`
Expected: All 8 tests pass

- [ ] **Step 5: Commit**

```bash
git add src/services/api/registry.rs src/services/api/mod.rs
git commit -m "feat(api): add ProviderRegistry with lazy creation and caching"
```

---

### Task 3: Wire registry into QueryContext and ToolUseContext

**Files:**
- Modify: `src/query/types.rs` (add `registry` field)
- Modify: `src/tool.rs` (add `registry` field to ToolUseContext + test helper)
- Modify: `src/query/engine.rs` (pass registry through to ToolUseContext + test helper)
- Modify: `src/services/agent_runtime.rs` (pass registry in child/fork contexts)

- [ ] **Step 1: Add `registry` field to QueryContext**

In `src/query/types.rs`, add after the `provider` field:

```rust
pub registry: Option<Arc<crate::services::api::registry::ProviderRegistry>>,
```

- [ ] **Step 2: Add `registry` field to ToolUseContext**

In `src/tool.rs`, add after the `provider` field in `ToolUseContext`:

```rust
pub registry: Option<Arc<crate::services::api::registry::ProviderRegistry>>,
```

In the `ToolUseContext::test()` helper, add:

```rust
registry: None,
```

- [ ] **Step 3: Pass registry in engine.rs execute_tool**

In `src/query/engine.rs` `execute_tool()`, add to the `ToolUseContext` construction (~line 444-464):

```rust
registry: ctx.registry.clone(),
```

- [ ] **Step 4: Pass registry in agent_runtime.rs**

In `src/services/agent_runtime.rs`, add to `build_child_context()` QueryContext (~line 112):

```rust
registry: None, // TODO: propagate parent_ctx.registry.clone() when subagents need multi-provider
```

In `build_fork_context()` QueryContext (~line 185):

```rust
registry: None,
```

- [ ] **Step 5: Fix all QueryContext construction sites**

Add `registry: None,` to every `QueryContext { .. }` in:
- `src/tui/run.rs` (~line 641 and ~line 750) — will become `Some(registry.clone())` in Task 5
- `src/entrypoints/print.rs` (~line 72)
- `src/query/engine.rs` `test_query_context()` (~line 483)

- [ ] **Step 6: Run full test suite**

Run: `cargo test`
Expected: All 553+ tests pass, no compile errors

- [ ] **Step 7: Commit**

```bash
git add src/query/types.rs src/tool.rs src/query/engine.rs src/services/agent_runtime.rs src/tui/run.rs src/entrypoints/print.rs
git commit -m "feat(api): thread registry field through QueryContext and ToolUseContext"
```

---

### Task 4: Wire registry into HookContext

**Files:**
- Modify: `src/services/hooks/types.rs` (add `registry` field)
- Modify: `src/query/engine.rs` (pass registry when constructing HookContext)

- [ ] **Step 1: Add `registry` field to HookContext**

In `src/services/hooks/types.rs`, modify `HookContext`:

```rust
pub struct HookContext {
    pub provider: Option<Arc<dyn crate::services::api::provider::LlmProvider>>,
    pub registry: Option<Arc<crate::services::api::registry::ProviderRegistry>>,
}
```

- [ ] **Step 2: Update HookContext construction in engine.rs**

In `src/query/engine.rs` (~line 304), update:

```rust
let hook_ctx = HookContext {
    provider: Some(ctx.provider.clone()),
    registry: ctx.registry.clone(),
};
```

- [ ] **Step 3: Run full test suite**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add src/services/hooks/types.rs src/query/engine.rs
git commit -m "feat(hooks): add registry to HookContext for model resolution"
```

---

### Task 5: Wire registry into main.rs and run_app

**Files:**
- Modify: `src/main.rs` (build registry from config, register _cli, pass to run_app)
- Modify: `src/tui/run.rs` (accept registry param, use in QueryContext)
- Modify: `src/entrypoints/print.rs` (accept registry param)

- [ ] **Step 1: Build registry in main.rs**

In `src/main.rs`, after loading config (~line 654), build the registry:

```rust
let config = AtomConfig::load();

// Build provider registry from config
let mut registry = crate::services::api::registry::ProviderRegistry::from_config(&config)
    .unwrap_or_else(|e| {
        eprintln!("Warning: Failed to load provider registry: {}", e);
        crate::services::api::registry::ProviderRegistry::new("_cli".into(), None)
    });
```

After `create_provider()` succeeds for CLI args, register as `_cli`:

```rust
// Register CLI provider in registry
if effective_provider.is_some() || effective_api_base.is_some() {
    registry.register("_cli".into(), crate::services::api::registry::ProviderConfig {
        provider_type: effective_provider.clone().unwrap_or_else(|| "openai".into()),
        api_base: effective_api_base.clone(),
        api_key: effective_api_key.clone(),
        models: HashMap::new(),
    });
    registry.set_default("_cli".into(), effective_model.clone());
}

let registry = Arc::new(registry);
```

- [ ] **Step 2: Add `registry` param to `run_app()`**

In `src/tui/run.rs`, add parameter:

```rust
pub async fn run_app(
    provider: Option<Arc<dyn LlmProvider>>,
    registry: Arc<crate::services::api::registry::ProviderRegistry>,
    // ... rest unchanged
```

Update both `QueryContext` constructions to use `registry: Some(registry.clone())`.

- [ ] **Step 3: Update print mode**

In `src/entrypoints/print.rs`, add `registry` parameter and pass to QueryContext.

- [ ] **Step 4: Update main.rs call sites**

Pass `registry.clone()` to both `run_app()` and `run_print_mode()`.

- [ ] **Step 5: Run full test suite**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 6: Manual smoke test**

Run: `cargo run -- --provider openrouter --api-base https://openrouter.ai/api/v1 --api-key YOUR_KEY --model anthropic/claude-sonnet-4-20250514 --print "hello"`
Expected: Works as before (backward compatible)

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/tui/run.rs src/entrypoints/print.rs
git commit -m "feat(api): wire ProviderRegistry into main, run_app, and print mode"
```

---

### Task 6: from_config with legacy fallback + integration test

**Files:**
- Modify: `src/services/api/registry.rs` (add legacy config handling)

- [ ] **Step 1: Write failing test for legacy config (no providers section)**

```rust
#[test]
fn test_from_config_legacy_no_providers() {
    let json = r#"{
        "provider": "openrouter",
        "api_base": "https://openrouter.ai/api/v1",
        "api_key": "sk-test",
        "default_model": "fast",
        "models": { "fast": "google/gemma-3-27b-it:free" }
    }"#;
    let config: crate::services::config::AtomConfig = serde_json::from_str(json).unwrap();
    let registry = ProviderRegistry::from_config(&config).unwrap();

    let provider = registry.resolve("fast").unwrap();
    assert_eq!(provider.model_info().id, "google/gemma-3-27b-it:free");
}

#[test]
fn test_from_config_with_providers() {
    let json = r#"{
        "providers": {
            "or": {
                "type": "openai",
                "api_base": "https://openrouter.ai/api/v1",
                "api_key": "sk-test",
                "models": { "sonnet": "anthropic/claude-sonnet-4-20250514" }
            }
        },
        "default_provider": "or",
        "default_model": "sonnet"
    }"#;
    let config: crate::services::config::AtomConfig = serde_json::from_str(json).unwrap();
    let registry = ProviderRegistry::from_config(&config).unwrap();

    let provider = registry.default_provider().unwrap();
    assert_eq!(provider.model_info().id, "anthropic/claude-sonnet-4-20250514");
}

#[test]
fn test_from_config_reserved_name_rejected() {
    let json = r#"{
        "providers": {
            "_bad": {
                "type": "openai",
                "api_base": "http://localhost:8000/v1"
            }
        },
        "default_provider": "_bad"
    }"#;
    let config: crate::services::config::AtomConfig = serde_json::from_str(json).unwrap();
    let err = ProviderRegistry::from_config(&config).unwrap_err();
    assert!(err.to_string().contains("reserved"));
}
```

- [ ] **Step 2: Run tests to verify the legacy test fails**

Run: `cargo test --lib api::registry::tests::test_from_config_legacy -- --nocapture`
Expected: FAIL — legacy config path not yet handled

- [ ] **Step 3: Add legacy config handling to `from_config()`**

In `ProviderRegistry::from_config()`, after the providers loop, add:

```rust
// Legacy fallback: if no `providers` section, build from top-level fields
if config.providers.is_none() {
    if let Some(ref api_base) = config.api_base_for_provider() {
        let mut models = HashMap::new();
        for (alias, raw_id) in &config.models {
            models.insert(alias.clone(), ModelEntry {
                id: raw_id.clone(),
                context_window: None,
                max_output_tokens: None,
            });
        }
        let provider_name = config.provider.clone().unwrap_or_else(|| "default".into());
        registry.register(provider_name.clone(), ProviderConfig {
            provider_type: "openai".into(),
            api_base: Some(api_base.clone()),
            api_key: config.api_key.clone(),
            models,
        });
        registry.set_default(provider_name, config.default_model.clone());
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib api::registry::tests -- --nocapture`
Expected: All registry tests pass (including legacy + providers + reserved name)

- [ ] **Step 5: Commit**

```bash
git add src/services/api/registry.rs
git commit -m "feat(api): add legacy config fallback to ProviderRegistry::from_config"
```

---

### Task 7: Final integration — full test suite + cleanup

**Files:**
- No new files — verification only

- [ ] **Step 1: Run full test suite**

Run: `cargo test`
Expected: All 553+ tests pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy -- -D warnings 2>&1 | head -50`
Expected: No warnings

- [ ] **Step 3: Verify compilation**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Compiles successfully

- [ ] **Step 4: Final commit if any fixups needed**

Only if clippy or tests required changes.
