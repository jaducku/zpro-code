# Provider Registry Design Spec

## Problem

atom-code-rs currently supports only a single LLM provider per session, created at startup from CLI args. There is no way to:
- Register multiple providers (OpenRouter, vLLM, Ollama) in config
- Reference different providers from hooks, agents, or other subsystems
- Switch models across providers at runtime

## Design

### Config Schema

Location: `~/.zpro-code/config.json`

```json
{
  "providers": {
    "or": {
      "type": "openai",
      "api_base": "https://openrouter.ai/api/v1",
      "api_key": "sk-or-...",
      "models": {
        "sonnet": { "id": "anthropic/claude-sonnet-4-20250514" },
        "haiku": { "id": "anthropic/claude-haiku" },
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
    },
    "ollama": {
      "type": "openai",
      "api_base": "http://localhost:11434/v1",
      "models": {
        "llama": { "id": "llama3.2" }
      }
    }
  },
  "default_provider": "or",
  "default_model": "sonnet"
}
```

### Model Entry

```rust
pub struct ModelEntry {
    pub id: String,                      // raw model ID sent to API
    pub context_window: Option<u64>,     // override, default 128_000
    pub max_output_tokens: Option<u32>,  // override, default 4096
}
```

Supports two formats in JSON for convenience:
- String shorthand: `"sonnet": "anthropic/claude-sonnet-4-20250514"` (no overrides)
- Object form: `"sonnet": { "id": "...", "context_window": 200000 }` (with overrides)

### Reference Syntax

Format: `"provider:model_alias_or_raw_id"`

| Input | Provider | Model |
|-------|----------|-------|
| `"or:sonnet"` | `or` | alias → `anthropic/claude-sonnet-4-20250514` |
| `"or:anthropic/claude-haiku"` | `or` | raw ID (alias lookup fails, use as-is) |
| `"vllm:qwen"` | `vllm` | alias → `Qwen/Qwen2.5-72B` |
| `"sonnet"` | default (`or`) | alias → `anthropic/claude-sonnet-4-20250514` |

Parsing: `split_once(':')` — first colon is the separator. Fallback: if the left side is not a known provider name, treat the entire string as a model reference against the default provider. This handles raw model IDs containing colons (e.g., `"google/gemma-3-27b-it:free"` resolves as default provider + raw ID, not provider=`"google/gemma-3-27b-it"`).

### ProviderRegistry

```rust
pub struct ProviderConfig {
    pub provider_type: String,           // currently only "openai" (ChatCompletionsProvider)
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub models: HashMap<String, ModelEntry>,
}

pub struct ProviderRegistry {
    configs: HashMap<String, ProviderConfig>,
    cache: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
    default_provider: String,
    default_model: Option<String>,
}
```

#### Key methods

```rust
impl ProviderRegistry {
    /// Build from AtomConfig
    pub fn from_config(config: &AtomConfig) -> Result<Self>;

    /// Resolve "provider:model" spec → Arc<dyn LlmProvider>
    /// Lazy-creates and caches provider instances.
    /// Cache key: "provider_name:resolved_model_id"
    /// On cache miss: acquires write lock, uses entry API to avoid
    /// double-creation in concurrent calls.
    pub fn resolve(&self, spec: &str) -> Result<Arc<dyn LlmProvider>>;

    /// Get the default provider+model
    pub fn default_provider(&self) -> Result<Arc<dyn LlmProvider>>;

    /// Register a provider config at runtime (for CLI args).
    /// Must be called BEFORE wrapping in Arc<ProviderRegistry>.
    pub fn register(&mut self, name: String, config: ProviderConfig);
}
```

#### Lazy creation

On first `resolve("or:sonnet")`:
1. Parse spec → provider_name=`"or"`, model_ref=`"sonnet"`
2. Lookup `configs["or"]` → `ProviderConfig`
3. Resolve alias: `config.models["sonnet"].id` → `"anthropic/claude-sonnet-4-20250514"`
4. Cache key: `"or:anthropic/claude-sonnet-4-20250514"`
5. Check `cache` — miss → create `ChatCompletionsProvider::new(api_base, api_key, resolved_model)`
6. Store in cache, return `Arc`

On subsequent calls with same resolved model: cache hit, return immediately.

Note: each provider:model combination is a **distinct `LlmProvider` instance**. `"or:sonnet"` and `"or:haiku"` create two separate `ChatCompletionsProvider` instances sharing the same api_base/api_key but differing in model and ModelInfo. This is required because `LlmProvider.model_info()` returns model-specific metadata (context_window, max_output_tokens) baked into the instance.

#### ModelEntry override application

When creating a `ChatCompletionsProvider` from a `ModelEntry` with overrides, the provider's `ModelInfo` is constructed with those values instead of the hardcoded defaults:

```rust
ModelInfo {
    id: resolved_model_id,
    name: resolved_model_id,
    context_window: model_entry.context_window.unwrap_or(128_000),
    max_output_tokens: model_entry.max_output_tokens.unwrap_or(4096),
    // ...
}
```

This requires extending `ChatCompletionsProvider::new()` to accept optional `ModelInfo` overrides, or adding a `new_with_model_info()` constructor.

### CLI Compatibility

When user runs `atom --provider openrouter --api-base ... --model sonnet`:

1. Load config.json → build `ProviderRegistry`
2. CLI args → register as `"_cli"` provider:
   ```rust
   registry.register("_cli", ProviderConfig {
       provider_type: cli.provider,
       api_base: Some(cli.api_base),
       api_key: cli.api_key,
       models: HashMap::new(),  // raw model ID used directly
   });
   ```
3. Set `default_provider = "_cli"`, `default_model = cli.model`
4. If no CLI args, use config.json's `default_provider` + `default_model`

Existing `create_provider()` function becomes a thin wrapper around `registry.resolve()`.

### Supported Provider Types

Currently only `"openai"` is supported in the registry, which uses `ChatCompletionsProvider`. This covers OpenRouter, vLLM, Ollama, and any OpenAI-compatible API — they differ only in `api_base` and `api_key`, not in protocol. The existing factory's "openrouter", "ollama", "custom" types are subsumed by explicit `api_base` in the config.

New provider types (e.g., native Anthropic API, Bedrock) can be added later by extending the match in the lazy creation logic.

### Codex OAuth

Excluded from registry for now. Codex uses `read_codex_token()` which reads OAuth tokens dynamically. When `--provider codex` is used via CLI, it registers as `_cli` provider with the token. Registry config does not support codex — this is a future extension when OAuth refresh logic is added.

### Reserved Names

Provider names starting with `_` are reserved for internal use (e.g., `_cli`). User-defined providers should not use this prefix.

### Injection Path

```
config.json + CLI args
    ↓
ProviderRegistry::from_config() + register("_cli", ...)
    ↓
Arc<ProviderRegistry> stored in run_app()
    ↓
QueryContext { provider: registry.default()?, registry: registry.clone() }
    ↓
ToolUseContext { provider, registry }
    ↓
Agent hook: ctx.registry.resolve("vllm:qwen")?
Subagent: ctx.registry.default()? (or specified model)
```

### Changes to Existing Types

**QueryContext** — add field:
```rust
pub registry: Option<Arc<ProviderRegistry>>,
```

**ToolUseContext** — add field:
```rust
pub registry: Option<Arc<ProviderRegistry>>,
```

Note: `Option` wrapping allows test contexts and subagents to pass `None` without requiring a real registry. This differs from the original design but is practically superior for backward compatibility.

**HookContext** — add field:
```rust
pub struct HookContext {
    pub provider: Option<Arc<dyn LlmProvider>>,
    pub registry: Option<Arc<ProviderRegistry>>,  // NEW: for model resolution in hooks
}
```

Prompt/Agent hooks with a `model` field use `registry.resolve(model)` to get the correct provider. Without the registry, the `model` field would be sent as a raw string to the current provider, which fails for cross-provider references like `"vllm:qwen"`.

**AppState** — no changes. Registry is not session state.

**AtomConfig** — add fields:
```rust
pub providers: Option<HashMap<String, ProviderConfigJson>>,
pub default_provider: Option<String>,
// existing: pub default_model: Option<String>
// existing: pub models: HashMap<String, String>  (kept for backward compat)
```

**ProviderConfigJson** — serde-friendly config type:
```rust
/// JSON representation of a provider config.
/// The `models` field supports both string shorthand and object form
/// via #[serde(deserialize_with = "...")] or #[serde(untagged)] on ModelEntry.
#[derive(Deserialize)]
pub struct ProviderConfigJson {
    #[serde(rename = "type")]
    pub provider_type: Option<String>,   // default: "openai"
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    pub models: Option<HashMap<String, ModelEntryJson>>,
}

/// Deserializes from either:
/// - String: "anthropic/claude-sonnet-4-20250514" (shorthand, no overrides)
/// - Object: { "id": "...", "context_window": 200000 } (with overrides)
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ModelEntryJson {
    Short(String),
    Full { id: String, context_window: Option<u64>, max_output_tokens: Option<u32> },
}
```

### Resolution Priority

`default_model` resolves against the default provider's `models` map. The top-level `models` map in AtomConfig is only used when no `providers` section exists (legacy mode). When both exist, the `providers` section takes precedence.

### Backward Compatibility

- If `config.json` has no `providers` field, registry is built from CLI args only (current behavior). The top-level `models` map becomes the default provider's aliases.
- `create_provider()` still works for simple single-provider use

### Cache Lifetime

Cache grows monotonically during a session. No eviction is needed since sessions are short-lived CLI processes.

### Error Handling

| Scenario | Behavior |
|----------|----------|
| Unknown provider name | `Err("Unknown provider: 'xyz'")` |
| Unknown model alias (not in models map) | Treat as raw model ID, pass through |
| Missing api_base | `Err("api_base required for provider 'xyz'")` |
| Provider server unreachable | Fails on first `create_message()`, not on `resolve()` |
| No providers configured + no CLI args | `Err("No provider configured")` |

### Testing

- Unit: `ProviderRegistry::resolve()` parsing and caching
- Unit: Config deserialization (string shorthand + object form for ModelEntry)
- Unit: CLI args → `_cli` provider registration
- Unit: Default provider/model fallback chain
- Unit: Colon-in-model-ID fallback (`"google/gemma-3-27b-it:free"` → default provider + raw ID)
- Integration: Multiple providers in config, resolve different specs
