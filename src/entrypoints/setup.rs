use std::collections::HashMap;
use std::io::{self, Write};

use crate::services::config::{AtomConfig, ProviderConfigJson};

/// Run first-time setup if no config exists.
/// Returns true if setup was completed, false if skipped.
pub fn run_setup_if_needed() -> bool {
    let config_path = crate::utils::paths::global_config_dir().join("config.json");
    if config_path.exists() {
        return false;
    }

    eprintln!();
    eprintln!("  Welcome to zpro-code! Let's set up your LLM provider.");
    eprintln!();
    eprintln!("  Choose a provider:");
    eprintln!("    1) OpenRouter (multi-model gateway, free models available)");
    eprintln!("    2) Custom endpoint (self-hosted, Ollama, vLLM, etc.)");
    eprintln!("    3) Skip setup (create sample config)");
    eprintln!();
    eprint!("  Enter choice [1/2/3]: ");
    io::stderr().flush().ok();

    let mut choice = String::new();
    if io::stdin().read_line(&mut choice).is_err() {
        return false;
    }

    match choice.trim() {
        "1" => {
            let config = setup_openrouter();
            if let Err(e) = config.save() {
                eprintln!("  Error saving config: {}", e);
                return false;
            }
        }
        "2" => {
            let config = setup_custom();
            if let Err(e) = config.save() {
                eprintln!("  Error saving config: {}", e);
                return false;
            }
        }
        _ => {
            if let Err(e) = AtomConfig::save_sample() {
                eprintln!("  Error saving sample config: {}", e);
                return false;
            }
            eprintln!("  Sample config saved to ~/.zpro-code/config.json");
            eprintln!("  Add your API key to get started.");
            eprintln!();
            return false;
        }
    }

    eprintln!();
    eprintln!("  Config saved to ~/.zpro-code/config.json");
    eprintln!("  Run 'zpro-code' to start!");
    eprintln!();
    true
}

fn setup_openrouter() -> AtomConfig {
    let mut providers = HashMap::new();
    let mut models = HashMap::new();
    let mut provider_models = Vec::new();

    eprintln!();
    eprintln!("  Get your API key at: https://openrouter.ai/keys");
    eprintln!();
    eprint!("  API key: ");
    io::stderr().flush().ok();

    let mut key = String::new();
    let api_key = if io::stdin().read_line(&mut key).is_ok() {
        let k = key.trim().to_string();
        if k.is_empty() {
            eprintln!("  (no key — add it later in config.json)");
            "YOUR_OPENROUTER_API_KEY".to_string()
        } else {
            eprintln!("  ✓ API key saved");
            k
        }
    } else {
        "YOUR_OPENROUTER_API_KEY".to_string()
    };

    // Register models
    eprintln!();
    eprintln!("  Register models (alias → model ID). Empty alias to finish.");
    eprintln!("  Browse models at: https://openrouter.ai/models");
    eprintln!();

    let mut default_model = None;
    loop {
        let hint = if default_model.is_none() { " (e.g. fast, smart, code)" } else { "" };
        eprint!("  Model alias{}: ", hint);
        io::stderr().flush().ok();
        let mut alias = String::new();
        if io::stdin().read_line(&mut alias).is_err() { break; }
        let alias = alias.trim().to_string();
        if alias.is_empty() { break; }

        eprint!("  Model ID for '{}': ", alias);
        io::stderr().flush().ok();
        let mut model_id = String::new();
        if io::stdin().read_line(&mut model_id).is_err() { break; }
        let model_id = model_id.trim().to_string();
        if model_id.is_empty() { continue; }

        provider_models.push(model_id.clone());
        models.insert(alias.clone(), format!("openrouter:{}", model_id));
        eprintln!("  ✓ {} → openrouter:{}", alias, model_id);
        eprintln!();

        if default_model.is_none() {
            default_model = Some(alias);
        }
    }

    providers.insert(
        "openrouter".to_string(),
        ProviderConfigJson {
            api_base: Some("https://openrouter.ai/api/v1".to_string()),
            api_key: Some(api_key),
            models: provider_models,
        },
    );

    if models.is_empty() {
        eprintln!("  No models registered. Add them later in config.json");
    } else {
        eprintln!();
        eprintln!("  Default model: {}", default_model.as_deref().unwrap_or("(none)"));
    }

    AtomConfig {
        default_model,
        providers: Some(providers),
        models,
        ..Default::default()
    }
}

fn setup_custom() -> AtomConfig {
    let mut providers = HashMap::new();
    let mut models = HashMap::new();

    eprintln!();
    eprint!("  API base URL (e.g. http://localhost:11434/v1): ");
    io::stderr().flush().ok();

    let mut url = String::new();
    let api_base = if io::stdin().read_line(&mut url).is_ok() {
        let u = url.trim().to_string();
        if u.is_empty() { None } else { Some(u) }
    } else {
        None
    };

    eprint!("  API key (leave empty if none): ");
    io::stderr().flush().ok();

    let mut key = String::new();
    let api_key = if io::stdin().read_line(&mut key).is_ok() {
        let k = key.trim().to_string();
        if k.is_empty() { None } else { Some(k) }
    } else {
        None
    };

    eprint!("  Model name (e.g. llama3, gpt-4o): ");
    io::stderr().flush().ok();

    let mut model = String::new();
    let mut provider_models = Vec::new();
    let default_model = if io::stdin().read_line(&mut model).is_ok() {
        let m = model.trim().to_string();
        if m.is_empty() {
            None
        } else {
            provider_models.push(m.clone());
            models.insert("default".to_string(), format!("custom:{}", m));
            Some("default".to_string())
        }
    } else {
        None
    };

    providers.insert(
        "custom".to_string(),
        ProviderConfigJson { api_base, api_key, models: provider_models },
    );

    AtomConfig {
        default_model,
        providers: Some(providers),
        models,
        ..Default::default()
    }
}
