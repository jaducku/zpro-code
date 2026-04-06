use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::commands::registry::CommandRegistry;
use crate::services::api::types::SystemBlock;
use crate::services::hooks::command_hook::HookEnv;
use crate::services::hooks::prompt_hook::build_prompt_input;
use crate::services::hooks::types::{HookContext, HookExecutionResult, HookOutcome};
use crate::query::engine::query;
use crate::query::types::{QueryContext, QueryEvent, PermissionAsk, UserQuestionAsk};
use crate::tool::Tool;
use crate::tools::get_builtin_tools;
use crate::tools::synthetic_output_tool::SyntheticOutputTool;
use crate::types::content::ContentBlock;
use crate::types::message::{ApiMessage, Role};
use crate::types::permissions::{PermissionMode, ToolPermissionContext};

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
    let sot = SyntheticOutputTool::new(hook_response_schema())
        .expect("hook_response_schema is a valid JSON Schema");
    tools.push(Box::new(sot));
    tools
}

const AGENT_HOOK_SYSTEM_PROMPT: &str = "\
You are verifying a condition in a coding assistant hook. \
Your task is to verify the given condition using the available tools.\n\n\
Use as few steps as possible - be efficient and direct.\n\n\
When done, return your result using the StructuredOutput tool with:\n\
- ok: true if the condition is met\n\
- ok: false with reason if the condition is not met";

/// Execute an agent-based hook using a multi-turn LLM query.
pub async fn execute_agent_hook(
    prompt: &str,
    model: Option<&str>,
    timeout: Option<u64>,
    env: &HookEnv,
    hook_ctx: &HookContext,
) -> HookExecutionResult {
    // 1. Resolve provider — if model specified, try registry first
    let (provider, model_id) = match model {
        Some(m) => {
            if let Some(reg) = &hook_ctx.registry {
                match reg.resolve(m) {
                    Ok(p) => {
                        let id = p.model_info().id.clone();
                        (p.clone(), id)
                    }
                    Err(_) => match &hook_ctx.provider {
                        Some(p) => (p.clone(), m.to_string()),
                        None => return HookExecutionResult {
                            outcome: HookOutcome::NonBlockingError {
                                message: format!("Agent hook: cannot resolve model '{}' (no provider)", m),
                            },
                            additional_contexts: Vec::new(),
                            watch_paths: Vec::new(),
                            hook_specific: None,
                        },
                    },
                }
            } else {
                match &hook_ctx.provider {
                    Some(p) => (p.clone(), m.to_string()),
                    None => return HookExecutionResult {
                        outcome: HookOutcome::NonBlockingError {
                            message: "Agent hooks require LLM provider (not available)".into(),
                        },
                        additional_contexts: Vec::new(),
                        watch_paths: Vec::new(),
                        hook_specific: None,
                    },
                }
            }
        }
        None => match &hook_ctx.provider {
            Some(p) => {
                let id = p.model_info().id.clone();
                (p.clone(), id)
            }
            None => return HookExecutionResult {
                outcome: HookOutcome::NonBlockingError {
                    message: "Agent hooks require LLM provider (not available)".into(),
                },
                additional_contexts: Vec::new(),
                watch_paths: Vec::new(),
                hook_specific: None,
            },
        },
    };

    // 2. Build prompt with $ARGUMENTS substitution
    let arguments_json = env.get("ATOM_TOOL_INPUT").cloned().unwrap_or_default();
    let processed_prompt = build_prompt_input(prompt, &arguments_json);

    // 3. Build tools
    let tools = build_agent_hook_tools();

    // 4. Build messages — single user message
    let messages = vec![ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text { text: processed_prompt }],
    }];

    // 5. Build QueryContext with throwaway channels
    let (event_tx, _event_rx) = mpsc::unbounded_channel::<QueryEvent>();
    let (permission_tx, _perm_rx) = mpsc::unbounded_channel::<PermissionAsk>();
    let (question_tx, _q_rx) = mpsc::unbounded_channel::<UserQuestionAsk>();
    let abort_signal = CancellationToken::new();

    let ctx = QueryContext {
        messages,
        tools,
        system_prompt: Some(vec![SystemBlock::Text {
            text: AGENT_HOOK_SYSTEM_PROMPT.to_string(),
        }]),
        model: model_id,
        max_tokens: 4096,
        permission_ctx: ToolPermissionContext {
            mode: PermissionMode::DontAsk,
            ..Default::default()
        },
        permission_tx,
        question_tx,
        hooks: HashMap::new(),
        cwd: PathBuf::from(env.get("ATOM_CWD").cloned().unwrap_or_else(|| "/tmp".into())),
        max_turns: Some(MAX_AGENT_HOOK_TURNS as u32),
        max_budget_usd: None,
        commands: Arc::new(CommandRegistry::empty()),
        provider,
        registry: hook_ctx.registry.clone(),
        app_state: Arc::new(tokio::sync::RwLock::new(crate::state::AppState::default())),
        mcp_manager: None,
        agent_depth: None,
        query_source: Some("hook_agent".into()),
    };

    // 6. Execute with timeout
    let timeout_ms = timeout.unwrap_or(60_000);
    let timeout_duration = std::time::Duration::from_millis(timeout_ms);

    let query_result = tokio::time::timeout(
        timeout_duration,
        query(ctx, event_tx, abort_signal),
    )
    .await;

    // 7. Process result
    match query_result {
        Err(_elapsed) => HookExecutionResult {
            outcome: HookOutcome::NonBlockingError {
                message: "Agent hook timed out".into(),
            },
            additional_contexts: Vec::new(),
            watch_paths: Vec::new(),
            hook_specific: None,
        },
        Ok(Err(e)) => HookExecutionResult {
            outcome: HookOutcome::NonBlockingError {
                message: format!("Agent hook query error: {}", e),
            },
            additional_contexts: Vec::new(),
            watch_paths: Vec::new(),
            hook_specific: None,
        },
        Ok(Ok(result)) => {
            match extract_structured_output(&result.messages) {
                Some(output) => match parse_agent_output(&output) {
                    Ok(()) => HookExecutionResult {
                        outcome: HookOutcome::Success,
                        additional_contexts: Vec::new(),
                        watch_paths: Vec::new(),
                        hook_specific: None,
                    },
                    Err(reason) => HookExecutionResult {
                        outcome: HookOutcome::Blocking { reason },
                        additional_contexts: Vec::new(),
                        watch_paths: Vec::new(),
                        hook_specific: None,
                    },
                },
                None => HookExecutionResult {
                    outcome: HookOutcome::NonBlockingError {
                        message: "Agent hook did not return structured output".into(),
                    },
                    additional_contexts: Vec::new(),
                    watch_paths: Vec::new(),
                    hook_specific: None,
                },
            }
        }
    }
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

    #[tokio::test]
    async fn test_execute_agent_hook_no_provider() {
        use std::collections::HashMap;
        use crate::services::hooks::types::{HookContext, HookOutcome};

        let env: HashMap<String, String> = HashMap::new();
        let hook_ctx = HookContext { provider: None, registry: None };
        let result = super::execute_agent_hook(
            "check something",
            None,
            None,
            &env,
            &hook_ctx,
        ).await;
        assert!(
            matches!(result.outcome, HookOutcome::NonBlockingError { .. }),
            "Expected NonBlockingError when no provider, got {:?}",
            result.outcome
        );
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
