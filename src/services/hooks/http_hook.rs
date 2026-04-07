use std::collections::HashMap;
use std::time::Duration;

use regex::Regex;

/// Remove CR, LF, and NUL characters from header values to prevent injection.
pub fn sanitize_header_value(value: &str) -> String {
    value.chars().filter(|c| *c != '\r' && *c != '\n' && *c != '\0').collect()
}

/// Replace `$VAR_NAME` patterns with values from env, but only for vars in allowed_vars.
/// Any `$VAR` not in allowed_vars gets replaced with empty string.
pub fn interpolate_env_vars(
    template: &str,
    env: &HashMap<String, String>,
    allowed_vars: &[String],
) -> String {
    let re = Regex::new(r"\$([A-Z_][A-Z0-9_]*)").unwrap();
    re.replace_all(template, |caps: &regex::Captures| {
        let var_name = &caps[1];
        if allowed_vars.iter().any(|v| v == var_name) {
            env.get(var_name).cloned().unwrap_or_default()
        } else {
            String::new()
        }
    })
    .into_owned()
}

#[derive(Debug)]
pub struct HttpHookResult {
    pub status: u16,
    pub body: String,
}

const DEFAULT_TIMEOUT_SECS: u64 = 600;

/// Execute an HTTP POST hook.
///
/// Sends `input_json` as the POST body to `url`, with `Content-Type: application/json`.
/// Header values are interpolated for allowed env vars, then sanitized.
pub async fn execute_http_hook(
    url: &str,
    headers: &HashMap<String, String>,
    allowed_env_vars: &[String],
    env: &HashMap<String, String>,
    input_json: &str,
    timeout_secs: Option<u64>,
) -> Result<HttpHookResult, String> {
    let client = reqwest::Client::new();
    let timeout_duration = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));

    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .timeout(timeout_duration)
        .body(input_json.to_string());

    for (key, value) in headers {
        let interpolated = interpolate_env_vars(value, env, allowed_env_vars);
        let sanitized = sanitize_header_value(&interpolated);
        request = request.header(key.as_str(), sanitized);
    }

    let response = request.send().await.map_err(|e| format!("HTTP request failed: {}", e))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    Ok(HttpHookResult { status, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_header_value() {
        assert_eq!(sanitize_header_value("Bearer tok\r\nen"), "Bearer token");
        assert_eq!(sanitize_header_value("normal"), "normal");
        assert_eq!(sanitize_header_value("has\0null"), "hasnull");
    }

    #[test]
    fn test_interpolate_env_vars() {
        let mut env = HashMap::new();
        env.insert("TOKEN".to_string(), "secret123".to_string());
        let allowed = vec!["TOKEN".to_string()];
        assert_eq!(
            interpolate_env_vars("Bearer $TOKEN", &env, &allowed),
            "Bearer secret123"
        );
    }

    #[test]
    fn test_interpolate_env_vars_blocks_unallowed() {
        let mut env = HashMap::new();
        env.insert("SECRET".to_string(), "hidden".to_string());
        let allowed: Vec<String> = vec![];
        assert_eq!(
            interpolate_env_vars("Bearer $SECRET", &env, &allowed),
            "Bearer "
        );
    }

    #[test]
    fn test_interpolate_env_vars_multiple() {
        let mut env = HashMap::new();
        env.insert("A".to_string(), "1".to_string());
        env.insert("B".to_string(), "2".to_string());
        let allowed = vec!["A".to_string(), "B".to_string()];
        assert_eq!(interpolate_env_vars("$A-$B", &env, &allowed), "1-2");
    }
}
