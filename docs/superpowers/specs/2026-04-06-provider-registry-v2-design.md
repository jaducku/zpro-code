# Provider Registry v2 Design Spec — Model-Centric Config

## Problem

The v1 registry uses a provider-centric config where models are nested under providers. This doesn't match the mental model of "I have these LLMs available" — users think in terms of models, not provider endpoints.

## Design

### Config Schema

Location: `~/.zpro-code/config.json`

```json
{
  "providers": {
    "or": {
      "api_base": "https://openrouter.ai/api/v1",
      "api_key": "sk-or-..."
    },
    "ollama": {
      "api_base": "http://localhost:11434/v1"
    }
  },
  "llms": {
    "sonnet": { "provider": "or", "model_id": "anthropic/claude-sonnet-4-20250514" },
    "haiku": { "provider": "or", "model_id": "anthropic/claude-haiku" },
    "qwen": { "api_base": "http://localhost:8000/v1", "model_id": "Qwen/Qwen2.5-72B" },
    "llama": { "provider": "ollama", "model_id": "llama3.2" },
    "flash": {
      "provider": "or",
      "model_id": "google/gemini-2.5-flash",
      "context_window": 1000000,
      "max_output_tokens": 65536
    }
  },
  "default_model": "sonnet"
}
```

### Types

**Provider template** (connection info only, no models):
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfigJson {
    pub api_base: Option<String>,
    pub api_key: Option<String>,
}
```

Note: `type` field removed — all providers are OpenAI-compatible (ChatCompletionsProvider). The `type` field was unused in v1 since only "openai" was supported.

**LLM entry** (top-level model registration):
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct LlmConfigJson {
    /// Reference to a provider in the `providers` map (for connection info)
    pub provider: Option<String>,
    /// Raw model ID sent to the API
    pub model_id: String,
    /// Inline connection info (used when no provider reference)
    pub api_base: Option<String>,
    pub api_key: Option<String>,
    /// Optional overrides
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u32>,
}
```

Connection info resolution for each LLM entry:
- If `provider` is set: look up `providers[provider]` for `api_base` and `api_key`
- If `provider` is not set: use inline `api_base` and `api_key` from the LLM entry itself
- No override/merge: it's one or the other

**AtomConfig changes:**
```rust
pub struct AtomConfig {
    // ... existing fields unchanged ...
    pub providers: Option<HashMap<String, ProviderConfigJson>>,  // CHANGED: no models inside
    pub llms: Option<HashMap<String, LlmConfigJson>>,            // NEW
    pub default_provider: Option<String>,                         // KEEP for legacy
    // existing: pub models: HashMap<String, String>              // KEEP as-is (legacy)
    // existing: pub default_model: Option<String>                // KEEP as-is
}
```

### Reference Syntax

| Input | Resolution |
|-------|------------|
| `"sonnet"` | Look up `llms["sonnet"]` → provider ref or inline → resolve |
| `"or:anthropic/claude-haiku"` | Ad-hoc: provider `"or"` + raw model ID `"anthropic/claude-haiku"` |
| `"fast"` | Falls back to legacy `models["fast"]` → raw ID on default provider |

Parsing logic:
1. Look up `spec` in registered LLM names → if found, use it
2. `split_once(':')` — if left side is a known provider, use as ad-hoc `provider:raw_model_id`
3. Fall back to default provider + treat spec as raw model ID

### ProviderRegistry changes

```rust
pub struct ProviderRegistry {
    /// Provider connection templates (api_base, api_key)
    provider_templates: HashMap<String, ProviderTemplate>,
    /// Registered LLMs (name → resolved connection + model info)
    llm_configs: HashMap<String, ResolvedLlm>,
    /// Lazy-created provider instances, keyed by "api_base:model_id"
    cache: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
    default_model: Option<String>,
}

struct ProviderTemplate {
    api_base: String,
    api_key: Option<String>,
}

struct ResolvedLlm {
    api_base: String,
    api_key: Option<String>,
    model_id: String,
    context_window: Option<u64>,
    max_output_tokens: Option<u32>,
}
```

Key changes from v1:
- `configs` (per-provider) → `provider_templates` (connection only) + `llm_configs` (per-model)
- `default_provider` → `default_model` (models are first-class)
- Cache key: LLM entry name for named entries (e.g., `"sonnet"`), `"provider:raw_model_id"` for ad-hoc lookups. Using the entry name avoids collisions when the same endpoint+model is used with different API keys.

### from_config logic

1. Load `providers` → `provider_templates`
2. Load `llms` → for each entry:
   - If `provider` set: resolve `api_base`/`api_key` from `provider_templates[provider]`
   - If `provider` not set: use inline `api_base`/`api_key`
   - Error if both `provider` and inline `api_base` are set (ambiguous — pick one)
   - Error if neither provider ref nor inline `api_base` exists
   - Validate: `_` prefix on LLM names is reserved
   - Store as `ResolvedLlm`
3. Load legacy `models` → for each `(alias, raw_id)`:
   - Skip if `alias` already exists in `llm_configs` (`llms` wins)
   - If `_cli` provider template exists: convert to `ResolvedLlm` using `_cli` connection info + raw_id as model_id
   - Otherwise: skip (these entries are inert until a CLI provider is registered)
4. Set `default_model` from config

### CLI compatibility

Same as v1: CLI args register as `_cli` entries.

When `--model sonnet` is passed and `"sonnet"` exists in `llms`, the registered LLM config is used directly (not the CLI provider).

When `--model unknown-model` is passed and it's not in `llms`, it uses the `_cli` provider's connection info + raw model ID.

### resolve() logic

```
resolve("sonnet"):
  1. llm_configs["sonnet"] exists → ResolvedLlm { api_base, api_key, model_id, ... }
  2. cache key = "sonnet" (the LLM entry name)
  3. cache hit → return
  4. cache miss → ChatCompletionsProvider::new(api_base, api_key, model_id) → cache → return

resolve("or:anthropic/claude-haiku"):
  1. llm_configs["or:anthropic/claude-haiku"] not found
  2. split_once(':') → "or" is a known provider template
  3. ad-hoc ResolvedLlm { api_base: or.api_base, api_key: or.api_key, model_id: "anthropic/claude-haiku" }
  4. cache key = "or:anthropic/claude-haiku"
  5. cache + return

resolve("gpt-4o") with _cli provider registered:
  1. llm_configs["gpt-4o"] not found
  2. No colon or left side not a known provider
  3. Fallback: _cli provider template exists → ad-hoc ResolvedLlm using _cli connection + "gpt-4o" as raw model_id
  4. cache key = "_cli:gpt-4o"
  5. cache + return

resolve("gpt-4o") with NO _cli provider:
  1-2. Same as above
  3. No _cli provider, no fallback
  4. Error: "Unknown model: 'gpt-4o'. Register it in llms or provide a CLI provider."
```

### Error Handling

| Scenario | Behavior |
|----------|----------|
| LLM entry references unknown provider | Error at `from_config` time |
| LLM entry has no provider ref and no api_base | Error at `from_config` time |
| LLM entry has both `provider` and `api_base` | Error at `from_config` time (ambiguous) |
| Unknown model name, _cli exists | Fallback to _cli provider + raw model ID |
| Unknown model name, no _cli | Error: "Unknown model: 'xyz'" |
| Ad-hoc with unknown provider name | Error: "Unknown provider: 'xyz'" |
| No default_model configured | Error when `default_provider()` called |
| Reserved `_` prefix in provider/llm names | Error at `from_config` time |

### Backward Compatibility

- `models: HashMap<String, String>` untouched — `resolve_model()`, `list_models()` still work
- Legacy config (no `providers`/`llms` sections) still works via `_cli` provider from CLI args
- Old v1 `providers` config with nested `models` is NOT supported — clean break (v1 was just committed, no external users yet)

### Testing

- Unit: LlmConfigJson deserialization
- Unit: from_config with provider references
- Unit: from_config with inline connection info
- Unit: from_config error cases (unknown provider ref, missing api_base)
- Unit: resolve registered model, ad-hoc provider:model, unknown model
- Unit: cache hit/miss (same name → same instance)
- Unit: from_config error when both provider and api_base set
- Unit: _cli fallback for unknown model names
- Unit: legacy models fallback
- Unit: reserved name rejection
