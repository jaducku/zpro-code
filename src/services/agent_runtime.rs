use crate::query::types::QueryContext;
use crate::tool::{Tool, ToolUseContext};
use crate::tools::get_builtin_tools;

/// Tools allowed for background agents (read-only + search)
const BACKGROUND_ALLOWED_TOOLS: &[&str] = &[
    "Read",
    "Glob",
    "Grep",
    "WebFetch",
    "WebSearch",
    "ToolSearch",
    "TodoWrite",
    "ListMcpResources",
    "ReadMcpResource",
];

/// Parameters for spawning an agent
pub struct AgentSpawnParams {
    pub prompt: String,
    pub description: String,
    pub model: Option<String>,
    pub run_in_background: bool,
}

impl AgentSpawnParams {
    pub fn from_input(input: &serde_json::Value) -> Result<Self, crate::types::AtomError> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::types::AtomError::Tool {
                tool_name: "Agent".into(),
                message: "Missing required 'prompt' parameter".into(),
            })?
            .to_string();

        let description = input
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("agent task")
            .to_string();

        let model = input.get("model").and_then(|v| v.as_str()).map(String::from);

        let run_in_background = input
            .get("run_in_background")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(Self { prompt, description, model, run_in_background })
    }
}

/// Result of a completed agent run
pub struct AgentResult {
    pub agent_id: String,
    pub output: String,
    pub is_error: bool,
}

/// Create a fresh set of tools for an agent.
pub fn get_agent_tools(is_background: bool) -> Vec<Box<dyn Tool>> {
    let all = get_builtin_tools();
    if !is_background {
        return all;
    }
    all.into_iter()
        .filter(|t| BACKGROUND_ALLOWED_TOOLS.contains(&t.name()))
        .collect()
}

/// Build a child QueryContext from the parent's ToolUseContext.
/// event_tx and abort_signal are passed separately to query().
pub fn build_child_context(
    params: &AgentSpawnParams,
    parent_ctx: &ToolUseContext,
    tools: Vec<Box<dyn Tool>>,
) -> QueryContext {
    use crate::types::content::ContentBlock;
    use crate::types::message::{ApiMessage, Role};

    let model = params.model.clone().unwrap_or_else(|| parent_ctx.model.clone());
    let max_tokens = parent_ctx.provider.model_info().max_output_tokens;

    let messages = vec![ApiMessage {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: params.prompt.clone(),
        }],
    }];

    QueryContext {
        messages,
        tools,
        system_prompt: Some(parent_ctx.system_prompt.clone()),
        model,
        max_tokens,
        permission_ctx: parent_ctx.permission_ctx.clone(),
        permission_tx: parent_ctx.permission_tx.clone(),
        question_tx: parent_ctx.question_tx.clone(),
        hooks: crate::services::hooks::load_all_hooks(&parent_ctx.cwd),
        cwd: parent_ctx.cwd.clone(),
        max_turns: Some(50),
        max_budget_usd: None,
        commands: parent_ctx.commands.clone(),
        provider: parent_ctx.provider.clone(),
        app_state: parent_ctx.app_state.clone(),
        mcp_manager: parent_ctx.mcp_manager.clone(),
        agent_depth: Some(parent_ctx.agent_depth + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_agent_tools_foreground_includes_all() {
        let tools = get_agent_tools(false);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"Agent"), "foreground should include Agent");
        assert!(names.contains(&"Write"), "foreground should include Write");
        assert!(names.contains(&"Read"), "foreground should include Read");
    }

    #[test]
    fn test_get_agent_tools_background_is_restricted() {
        let tools = get_agent_tools(true);
        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"Read"), "background should include Read");
        assert!(names.contains(&"Grep"), "background should include Grep");
        assert!(names.contains(&"Glob"), "background should include Glob");
        assert!(!names.contains(&"Agent"), "background should exclude Agent");
        assert!(!names.contains(&"Write"), "background should exclude Write");
        assert!(!names.contains(&"Edit"), "background should exclude Edit");
    }

    #[test]
    fn test_agent_spawn_params_from_input() {
        let input = serde_json::json!({
            "prompt": "Fix the bug",
            "description": "fix bug",
            "model": "gpt-4o",
            "run_in_background": true
        });
        let params = AgentSpawnParams::from_input(&input).unwrap();
        assert_eq!(params.prompt, "Fix the bug");
        assert_eq!(params.description, "fix bug");
        assert_eq!(params.model, Some("gpt-4o".into()));
        assert!(params.run_in_background);
    }

    #[test]
    fn test_agent_spawn_params_defaults() {
        let input = serde_json::json!({ "prompt": "Do something" });
        let params = AgentSpawnParams::from_input(&input).unwrap();
        assert_eq!(params.description, "agent task");
        assert_eq!(params.model, None);
        assert!(!params.run_in_background);
    }

    #[test]
    fn test_agent_spawn_params_missing_prompt() {
        let input = serde_json::json!({});
        assert!(AgentSpawnParams::from_input(&input).is_err());
    }

    #[test]
    fn test_build_child_context() {
        let parent_ctx = ToolUseContext::test(serde_json::json!({}));
        let params = AgentSpawnParams {
            prompt: "test prompt".into(),
            description: "test".into(),
            model: Some("custom-model".into()),
            run_in_background: false,
        };
        let tools = get_agent_tools(false);
        let child = build_child_context(&params, &parent_ctx, tools);

        assert_eq!(child.model, "custom-model");
        assert_eq!(child.messages.len(), 1);
        assert_eq!(child.max_turns, Some(50));
        assert_eq!(child.agent_depth, Some(1));
    }

    #[test]
    fn test_build_child_context_inherits_parent_model() {
        let parent_ctx = ToolUseContext::test(serde_json::json!({}));
        let params = AgentSpawnParams {
            prompt: "test".into(),
            description: "test".into(),
            model: None,
            run_in_background: false,
        };
        let tools = get_agent_tools(false);
        let child = build_child_context(&params, &parent_ctx, tools);

        assert_eq!(child.model, "test"); // inherited from parent_ctx.model
    }
}
