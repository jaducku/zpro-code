use super::codex_auth::read_codex_token;
use super::openai_provider::OpenAiCompatibleProvider;
use super::provider::LlmProvider;

/// Create an LLM provider based on the provider type and configuration.
///
/// Provider types:
/// - "codex": Uses Codex CLI OAuth token with OpenAI API (api.openai.com)
/// - "openai" or None: Uses OpenAI-compatible API with api_base
pub fn create_provider(
    provider_type: Option<&str>,
    api_base: Option<&str>,
    api_key: Option<&str>,
    model: Option<&str>,
) -> anyhow::Result<Box<dyn LlmProvider>> {
    match provider_type {
        Some("codex") => {
            let token = read_codex_token()?;
            let model = model.unwrap_or("gpt-4o");
            Ok(Box::new(OpenAiCompatibleProvider::new(
                "https://api.openai.com/v1".into(),
                Some(token),
                model.into(),
            )))
        }
        Some("openai") | None => {
            let base = api_base.ok_or_else(|| {
                anyhow::anyhow!(
                    "--api-base is required. Example: --api-base https://openrouter.ai/api/v1"
                )
            })?;
            Ok(Box::new(OpenAiCompatibleProvider::new(
                base.into(),
                api_key.map(|s| s.to_string()),
                model.unwrap_or("default").into(),
            )))
        }
        Some(other) => Err(anyhow::anyhow!(
            "Unknown provider: '{}'. Available: codex, openai",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_provider_openai_requires_api_base() {
        let result = create_provider(Some("openai"), None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_provider_openai_with_base() {
        let result = create_provider(
            Some("openai"),
            Some("https://api.example.com/v1"),
            Some("sk-test"),
            Some("gpt-4"),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_provider_none_requires_api_base() {
        let result = create_provider(None, None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_provider_unknown() {
        let result = create_provider(Some("bedrock"), None, None, None);
        let err = result.err().expect("should be an error");
        assert!(err.to_string().contains("Unknown provider"));
    }

    #[test]
    fn test_create_provider_codex_with_env() {
        std::env::set_var("CODEX_TOKEN", "test-token-for-factory");
        let result = create_provider(Some("codex"), None, None, None);
        std::env::remove_var("CODEX_TOKEN");
        assert!(result.is_ok());
    }
}
