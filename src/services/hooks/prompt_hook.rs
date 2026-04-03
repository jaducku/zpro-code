/// Replace `$ARGUMENTS` in a prompt template with the actual JSON arguments.
pub fn build_prompt_input(prompt_template: &str, arguments_json: &str) -> String {
    prompt_template.replace("$ARGUMENTS", arguments_json)
}

/// Parse a prompt hook response JSON. Expects `{"ok": true/false, "reason": "..."}`.
///
/// - `ok=true` -> `Ok(())`
/// - `ok=false` -> `Err(reason)` (falls back to a default message)
pub fn parse_prompt_response(response: &str) -> Result<(), String> {
    let v: serde_json::Value =
        serde_json::from_str(response).map_err(|e| format!("Invalid JSON: {e}"))?;
    match v.get("ok").and_then(|v| v.as_bool()) {
        Some(true) => Ok(()),
        _ => {
            let reason = v
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("Blocked by prompt hook");
            Err(reason.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_prompt_hook_input() {
        let template = "Review this tool call: $ARGUMENTS";
        let args = r#"{"tool":"Bash","command":"rm -rf /"}"#;
        let result = build_prompt_input(template, args);
        assert_eq!(
            result,
            r#"Review this tool call: {"tool":"Bash","command":"rm -rf /"}"#
        );
    }

    #[test]
    fn test_parse_prompt_hook_response_ok() {
        let response = r#"{"ok": true}"#;
        assert!(parse_prompt_response(response).is_ok());
    }

    #[test]
    fn test_parse_prompt_hook_response_blocked() {
        let response = r#"{"ok": false, "reason": "dangerous"}"#;
        let err = parse_prompt_response(response).unwrap_err();
        assert_eq!(err, "dangerous");
    }
}
