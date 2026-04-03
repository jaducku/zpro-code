/// Read Codex CLI auth credentials from environment or standard file locations.
///
/// Search order:
/// 1. OPENAI_API_KEY environment variable (standard API key)
/// 2. CODEX_TOKEN environment variable
/// 3. ~/.codex/auth.json — prefers OPENAI_API_KEY field, then OAuth token
/// 4. ~/.config/codex/auth.json
///
/// Note: ChatGPT OAuth tokens (auth_mode: "chatgpt") require the ChatGPT
/// backend API which is Cloudflare-protected. API keys (sk-...) work with
/// the standard OpenAI API.
pub fn read_codex_token() -> anyhow::Result<String> {
    // 1. OPENAI_API_KEY env (highest priority — standard API key)
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        if !key.is_empty() {
            return Ok(key);
        }
    }

    // 2. CODEX_TOKEN env
    if let Ok(token) = std::env::var("CODEX_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // 3-4. File locations
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    let candidates = [
        format!("{}/.codex/auth.json", home),
        format!("{}/.config/codex/auth.json", home),
    ];

    for path in &candidates {
        if let Some(result) = read_token_from_file(path) {
            match result {
                AuthToken::ApiKey(key) => return Ok(key),
                AuthToken::OAuthToken(token) => {
                    // ChatGPT OAuth tokens don't work with api.openai.com/v1
                    // They need the ChatGPT backend API (Cloudflare protected).
                    // Still return it — the 401/429 error will be shown to user.
                    eprintln!(
                        "Warning: Using ChatGPT OAuth token. If you get API errors, \
                         set OPENAI_API_KEY environment variable with an API key instead."
                    );
                    return Ok(token);
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Codex auth not found.\n\
         Set OPENAI_API_KEY env var, or run 'codex auth login'."
    ))
}

enum AuthToken {
    ApiKey(String),
    OAuthToken(String),
}

fn read_token_from_file(path: &str) -> Option<AuthToken> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Prefer API key (works with standard OpenAI API)
    if let Some(key) = json["OPENAI_API_KEY"].as_str() {
        if !key.is_empty() {
            return Some(AuthToken::ApiKey(key.to_string()));
        }
    }

    // OAuth token (nested in "tokens" object)
    if let Some(token) = json["tokens"]["access_token"].as_str() {
        return Some(AuthToken::OAuthToken(token.to_string()));
    }

    // Flat access_token
    if let Some(token) = json["access_token"].as_str() {
        return Some(AuthToken::OAuthToken(token.to_string()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_codex_token_from_env() {
        std::env::set_var("CODEX_TOKEN", "env-token-123");
        let result = read_codex_token();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "env-token-123");
        std::env::remove_var("CODEX_TOKEN");
    }

    #[test]
    fn test_parse_auth_json() {
        let json = r#"{"access_token":"sk-test-12345","refresh_token":"rt-xyz"}"#;
        let value: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(value["access_token"].as_str().unwrap(), "sk-test-12345");
    }
}
