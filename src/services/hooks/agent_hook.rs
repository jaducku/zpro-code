/// Maximum number of agentic turns an agent hook may execute.
pub const MAX_AGENT_HOOK_TURNS: usize = 50;

/// Parse agent hook output JSON. Expects `{"ok": true/false, "reason": "..."}`.
///
/// - `ok=true` -> `Ok(())`
/// - `ok=false` -> `Err(reason)` (falls back to a default message)
pub fn parse_agent_output(output: &serde_json::Value) -> Result<(), String> {
    match output.get("ok").and_then(|v| v.as_bool()) {
        Some(true) => Ok(()),
        _ => {
            let reason = output
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("Blocked by agent hook");
            Err(reason.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_agent_output_ok() {
        let output = json!({"ok": true});
        assert!(parse_agent_output(&output).is_ok());
    }

    #[test]
    fn test_parse_agent_output_blocked() {
        let output = json!({"ok": false, "reason": "unsafe"});
        let err = parse_agent_output(&output).unwrap_err();
        assert_eq!(err, "unsafe");
    }
}
