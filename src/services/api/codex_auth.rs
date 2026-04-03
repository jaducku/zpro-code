/// Read Codex CLI OAuth token from environment or standard file locations.
///
/// Search order:
/// 1. CODEX_TOKEN environment variable
/// 2. ~/.codex/auth.json
/// 3. ~/.config/codex/auth.json
pub fn read_codex_token() -> anyhow::Result<String> {
    // 1. Env var
    if let Ok(token) = std::env::var("CODEX_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // 2-3. File locations
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();

    let candidates = [
        format!("{}/.codex/auth.json", home),
        format!("{}/.config/codex/auth.json", home),
    ];

    for path in &candidates {
        if let Some(token) = read_token_from_file(path) {
            return Ok(token);
        }
    }

    Err(anyhow::anyhow!(
        "Codex OAuth token not found.\n\
         Run 'codex auth login' first, or set CODEX_TOKEN environment variable."
    ))
}

fn read_token_from_file(path: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Try nested: { "tokens": { "access_token": "..." } } (Codex CLI format)
    if let Some(token) = json["tokens"]["access_token"].as_str() {
        return Some(token.to_string());
    }

    // Try flat: { "access_token": "..." }
    if let Some(token) = json["access_token"].as_str() {
        return Some(token.to_string());
    }

    // Try API key: { "OPENAI_API_KEY": "..." }
    if let Some(key) = json["OPENAI_API_KEY"].as_str() {
        if !key.is_empty() {
            return Some(key.to_string());
        }
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
