use std::io::{self, Write};

use crate::services::config::AtomConfig;

/// Run first-time setup if no config exists.
/// Returns true if setup was completed, false if skipped.
pub fn run_setup_if_needed() -> bool {
    let config_path = crate::utils::paths::global_config_dir().join("config.json");
    if config_path.exists() {
        return false;
    }

    eprintln!();
    eprintln!("  Welcome to atom-code! Let's set up your LLM provider.");
    eprintln!();
    eprintln!("  Choose a provider:");
    eprintln!("    1) OpenRouter (multi-model gateway, free models available)");
    eprintln!("    2) Custom endpoint (self-hosted, Ollama, vLLM, etc.)");
    eprintln!("    3) Skip setup (configure later with ~/.atom/config.json)");
    eprintln!();
    eprint!("  Enter choice [1/2/3]: ");
    io::stderr().flush().ok();

    let mut choice = String::new();
    if io::stdin().read_line(&mut choice).is_err() {
        return false;
    }

    let mut config = AtomConfig::default();

    match choice.trim() {
        "1" => setup_openrouter(&mut config),
        "2" => setup_custom(&mut config),
        _ => {
            eprintln!("  Skipped. Edit ~/.atom/config.json to configure later.");
            return false;
        }
    }

    // Save config
    if let Err(e) = config.save() {
        eprintln!("  Error saving config: {}", e);
        return false;
    }

    eprintln!();
    eprintln!("  Config saved to ~/.atom/config.json");
    eprintln!("  Run 'atom' to start!");
    eprintln!();
    true
}

fn setup_openrouter(config: &mut AtomConfig) {
    config.provider = Some("openrouter".into());

    eprintln!();
    eprintln!("  Get your API key at: https://openrouter.ai/keys");
    eprintln!();
    eprint!("  API key: ");
    io::stderr().flush().ok();

    let mut key = String::new();
    if io::stdin().read_line(&mut key).is_ok() {
        let key = key.trim().to_string();
        if !key.is_empty() {
            config.api_key = Some(key);
        }
    }

    // Default models
    config.default_model = Some("fast".into());
    config.models.insert("fast".into(), "google/gemma-3-27b-it:free".into());
    config.models.insert("smart".into(), "anthropic/claude-sonnet-4-20250514".into());
    config.models.insert("code".into(), "deepseek/deepseek-chat-v3-0324:free".into());

    eprintln!();
    eprintln!("  Registered models:");
    eprintln!("    fast  → google/gemma-3-27b-it:free (default)");
    eprintln!("    smart → anthropic/claude-sonnet-4-20250514");
    eprintln!("    code  → deepseek/deepseek-chat-v3-0324:free");
    eprintln!();
    eprintln!("  Change default: atom --model smart");
    eprintln!("  Add more: edit ~/.atom/config.json");
}

fn setup_custom(config: &mut AtomConfig) {
    config.provider = Some("custom".into());

    eprintln!();
    eprint!("  API base URL (e.g. http://localhost:11434/v1): ");
    io::stderr().flush().ok();

    let mut url = String::new();
    if io::stdin().read_line(&mut url).is_ok() {
        let url = url.trim().to_string();
        if !url.is_empty() {
            config.api_base = Some(url);
        }
    }

    eprint!("  API key (leave empty if none): ");
    io::stderr().flush().ok();

    let mut key = String::new();
    if io::stdin().read_line(&mut key).is_ok() {
        let key = key.trim().to_string();
        if !key.is_empty() {
            config.api_key = Some(key);
        }
    }

    eprint!("  Model name (e.g. llama3, gpt-4o): ");
    io::stderr().flush().ok();

    let mut model = String::new();
    if io::stdin().read_line(&mut model).is_ok() {
        let model = model.trim().to_string();
        if !model.is_empty() {
            config.default_model = Some("default".into());
            config.models.insert("default".into(), model);
        }
    }
}
