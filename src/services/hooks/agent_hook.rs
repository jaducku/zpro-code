use crate::tool::Tool;
use crate::tools::get_builtin_tools;
use crate::tools::synthetic_output_tool::SyntheticOutputTool;
use crate::types::content::ContentBlock;
use crate::types::message::ApiMessage;

/// Maximum number of agentic turns an agent hook may execute.
pub const MAX_AGENT_HOOK_TURNS: usize = 50;

/// Tools that agent hooks must not use (prevent recursion, deadlocks, irrelevant actions).
pub const AGENT_HOOK_DISALLOWED_TOOLS: &[&str] = &[
    "Agent",
    "AskUserQuestion",
    "EnterPlanMode",
    "ExitPlanMode",
    "EnterWorktree",
    "ExitWorktree",
    "SendMessage",
    "Skill",
    "TaskStop",
    "TaskOutput",
    "TeamCreate",
    "TeamDelete",
];

/// JSON Schema for agent hook structured output: `{ok: boolean, reason?: string}`.
pub fn hook_response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "ok": { "type": "boolean" },
            "reason": { "type": "string" }
        },
        "required": ["ok"]
    })
}

/// Build the tool set for an agent hook: all builtins minus disallowed, plus SyntheticOutputTool.
pub fn build_agent_hook_tools() -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = get_builtin_tools()
        .into_iter()
        .filter(|t| {
            !AGENT_HOOK_DISALLOWED_TOOLS.contains(&t.name()) && t.name() != "StructuredOutput"
        })
        .collect();
    // SyntheticOutputTool::new should not fail for our well-known schema
    if let Ok(sot) = SyntheticOutputTool::new(hook_response_schema()) {
        tools.push(Box::new(sot));
    }
    tools
}

/// Scan messages backwards for the last StructuredOutput tool_use input.
/// Returns the `input` JSON value if found.
pub fn extract_structured_output(messages: &[ApiMessage]) -> Option<serde_json::Value> {
    for msg in messages.iter().rev() {
        for block in msg.content.iter().rev() {
            if let ContentBlock::ToolUse { name, input, .. } = block {
                if name == "StructuredOutput" {
                    return Some(input.clone());
                }
            }
        }
    }
    None
}

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
    use crate::types::content::ContentBlock;
    use crate::types::message::{ApiMessage, Role};
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

    #[test]
    fn test_hook_response_schema() {
        let schema = super::hook_response_schema();
        // Must be a valid JSON Schema with "ok" required
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["ok"]["type"] == "boolean");
        assert!(schema["properties"]["reason"]["type"] == "string");
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("ok")));
    }

    #[test]
    fn test_build_agent_hook_tools_filters_disallowed() {
        let tools = super::build_agent_hook_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        // Disallowed tools must not be present
        for disallowed in super::AGENT_HOOK_DISALLOWED_TOOLS {
            assert!(
                !names.contains(disallowed),
                "Disallowed tool '{}' should be filtered out",
                disallowed
            );
        }
        // StructuredOutput must be present (from SyntheticOutputTool)
        assert!(names.contains(&"StructuredOutput"), "StructuredOutput tool must be present");
    }

    #[test]
    fn test_build_agent_hook_tools_has_core_tools() {
        let tools = super::build_agent_hook_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        // Core tools should survive filtering
        assert!(names.contains(&"Bash"));
        assert!(names.contains(&"Read"));
        assert!(names.contains(&"Glob"));
        assert!(names.contains(&"Grep"));
    }

    #[test]
    fn test_extract_structured_output_found() {
        let messages = vec![
            ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text { text: "verify condition".into() }],
            },
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "StructuredOutput".into(),
                    input: serde_json::json!({"ok": true}),
                }],
            },
        ];
        let result = super::extract_structured_output(&messages);
        assert!(result.is_some());
        assert_eq!(result.unwrap()["ok"], true);
    }

    #[test]
    fn test_extract_structured_output_not_found() {
        let messages = vec![
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::Text { text: "I'm done".into() }],
            },
        ];
        assert!(super::extract_structured_output(&messages).is_none());
    }

    #[test]
    fn test_extract_structured_output_takes_last() {
        let messages = vec![
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_1".into(),
                    name: "StructuredOutput".into(),
                    input: serde_json::json!({"ok": false, "reason": "first"}),
                }],
            },
            ApiMessage {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tu_2".into(),
                    name: "StructuredOutput".into(),
                    input: serde_json::json!({"ok": true}),
                }],
            },
        ];
        let result = super::extract_structured_output(&messages).unwrap();
        assert_eq!(result["ok"], true);
    }
}
