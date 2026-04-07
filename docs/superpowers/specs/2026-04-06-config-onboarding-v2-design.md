# Config Onboarding v2 Design Spec

## Overview

Update the first-run onboarding flow to generate v2 model-centric config format (`providers` + `llms`) instead of v1 flat format. Also create a usable sample config when setup is skipped, so users can start quickly by just adding an API key.

## Problems

1. `run_setup_if_needed()` generates v1 config (flat `provider`, `api_key`, `models` fields)
2. When setup is skipped or the app runs with a subcommand on first launch, no config file is created at all
3. Users have no starting point — must write config.json from scratch

## Design

### 1. Sample Config (`AtomConfig::save_sample()`)

New method that creates a ready-to-use config with OpenRouter free models pre-registered:

```json
{
  "providers": {
    "openrouter": {
      "api_base": "https://openrouter.ai/api/v1",
      "api_key": "YOUR_OPENROUTER_API_KEY"
    }
  },
  "llms": {
    "sonnet": {
      "provider": "openrouter",
      "model_id": "anthropic/claude-sonnet-4-20250514"
    },
    "flash": {
      "provider": "openrouter",
      "model_id": "google/gemini-2.5-flash-preview-04-17"
    },
    "free": {
      "provider": "openrouter",
      "model_id": "google/gemma-3-27b-it:free"
    }
  },
  "default_model": "free"
}
```

User only needs to replace `YOUR_OPENROUTER_API_KEY` with their key to start using the tool.

### 2. Setup Interactive Flow (v2)

**OpenRouter path:**
- Create `providers.openrouter` with `api_base` + user-provided `api_key`
- Model registration writes to `llms` map as `LlmConfigJson { provider: "openrouter", model_id }`
- `default_model` set to first registered model's alias
- v1 fields (`provider`, `api_key`, `models`) are NOT populated

**Custom endpoint path:**
- Create `providers.custom` with user-provided `api_base` + optional `api_key`
- Model registration writes to `llms` map as `LlmConfigJson { provider: "custom", model_id }`
- `default_model` set to first model alias

**Skip path:**
- Call `save_sample()` to create the sample config
- Print: "Sample config saved to ~/.zpro-code/config.json — add your API key to get started."

### 3. First-Run Guarantee

In `AtomConfig::load()`, after loading (or defaulting), if the config file did not exist, call `save_sample()` to ensure a config file always exists after the first load. This covers subcommand-first-run and any path that bypasses `run_setup_if_needed()`.

Alternative: keep this in `run_setup_if_needed()` skip path only, and also call `save_sample()` in `main.rs` before the main loop if no config exists. The simpler approach: just make the skip path in `run_setup_if_needed()` create the sample, and ensure `run_setup_if_needed()` is called before any config-dependent code.

Decision: Use the skip path approach — `run_setup_if_needed()` skip creates sample. For subcommand paths that bypass setup, add a one-line check in `AtomConfig::load()`: if file doesn't exist, save sample first then load.

### 4. Backward Compatibility

No breaking changes. `AtomConfig` deserializes both v1 and v2 formats via `serde(default)`. `ProviderRegistry::from_config()` already handles v2-first with v1 fallback.

## Files Changed

| File | Change |
|------|--------|
| `src/services/config/mod.rs` | Add `save_sample()` method, add auto-create in `load()` |
| `src/entrypoints/setup.rs` | Rewrite `setup_openrouter()` and `setup_custom()` to use v2 format, skip path calls `save_sample()` |

## Testing

- Unit test: `save_sample()` creates valid config with providers and llms
- Unit test: setup_openrouter generates v2 config (providers + llms, no v1 fields)
- Unit test: setup_custom generates v2 config
- Unit test: `load()` auto-creates sample when file missing (using temp dir)
