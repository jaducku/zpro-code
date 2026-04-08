use super::types::{AgentDefinition, AgentSource, AgentToolFilter};

/// Builtin agent definitions. These are hardcoded and have highest code-level authority,
/// but can be overridden by config (builtin_models.{name}).
/// Model priority: config override → code default → None (inherit from parent)

/// Create the general-purpose builtin agent.
fn general_purpose_agent(model_override: Option<String>) -> AgentDefinition {
    let code_default = None;
    let model = model_override.or(code_default);

    AgentDefinition {
        name: "general-purpose".to_string(),
        description: "General-purpose agent for complex reasoning and problem-solving tasks".to_string(),
        source: AgentSource::BuiltIn,
        system_prompt: None,
        tool_filter: AgentToolFilter::AllTools,
        disallowed_tools: Vec::new(),
        model,
        max_turns: None,
        permission_mode: None,
        isolation: None,
        query_source: "agent:builtin:general-purpose".to_string(),
    }
}

/// Create the Explore builtin agent.
fn explore_agent(model_override: Option<String>) -> AgentDefinition {
    let code_default = Some("haiku".to_string());
    let model = model_override.or(code_default);

    AgentDefinition {
        name: "Explore".to_string(),
        description: "Fast exploration agent for quickly scanning codebases and finding patterns".to_string(),
        source: AgentSource::BuiltIn,
        system_prompt: None,
        tool_filter: AgentToolFilter::AllTools,
        disallowed_tools: vec![
            "Agent".to_string(),
            "Edit".to_string(),
            "Write".to_string(),
            "Bash".to_string(),
            "FileEdit".to_string(),
            "FileWrite".to_string(),
        ],
        model,
        max_turns: Some(30),
        permission_mode: None,
        isolation: None,
        query_source: "agent:builtin:explore".to_string(),
    }
}

/// Create the Plan builtin agent.
fn plan_agent(model_override: Option<String>) -> AgentDefinition {
    let code_default = None; // Inherit from parent
    let model = model_override.or(code_default);

    AgentDefinition {
        name: "Plan".to_string(),
        description: "Agent for creating detailed implementation plans and architecture decisions".to_string(),
        source: AgentSource::BuiltIn,
        system_prompt: None,
        tool_filter: AgentToolFilter::AllTools,
        disallowed_tools: vec![
            "Agent".to_string(),
            "Edit".to_string(),
            "Write".to_string(),
            "Bash".to_string(),
            "FileEdit".to_string(),
            "FileWrite".to_string(),
        ],
        model,
        max_turns: Some(50),
        permission_mode: None,
        isolation: None,
        query_source: "agent:builtin:plan".to_string(),
    }
}

/// Create the Verification builtin agent.
fn verification_agent(model_override: Option<String>) -> AgentDefinition {
    let code_default = None; // Inherit from parent
    let model = model_override.or(code_default);

    AgentDefinition {
        name: "Verification".to_string(),
        description: "Agent for verifying implementations and ensuring quality standards".to_string(),
        source: AgentSource::BuiltIn,
        system_prompt: Some(
            "You are a verification specialist. Your role is to thoroughly verify implementations \
             against requirements, check for edge cases, and ensure quality standards are met.".to_string()
        ),
        tool_filter: AgentToolFilter::AllTools,
        disallowed_tools: Vec::new(),
        model,
        max_turns: Some(30),
        permission_mode: None,
        isolation: Some("worktree".to_string()),
        query_source: "agent:builtin:verification".to_string(),
    }
}

/// Create the claude-code-guide builtin agent.
fn claude_code_guide_agent(model_override: Option<String>) -> AgentDefinition {
    let code_default = None; // Inherit from parent
    let model = model_override.or(code_default);

    AgentDefinition {
        name: "claude-code-guide".to_string(),
        description: "Agent for answering questions about Claude Code features, CLI, and API usage".to_string(),
        source: AgentSource::BuiltIn,
        system_prompt: None,
        tool_filter: AgentToolFilter::AllowList(vec![
            "WebFetch".to_string(),
            "WebSearch".to_string(),
            "Read".to_string(),
            "Glob".to_string(),
            "Grep".to_string(),
        ]),
        disallowed_tools: Vec::new(),
        model,
        max_turns: Some(20),
        permission_mode: None,
        isolation: None,
        query_source: "agent:builtin:claude-code-guide".to_string(),
    }
}

/// Builtin model overrides from config (name → model).
/// Used to allow configuration-level override of hardcoded defaults.
#[derive(Debug, Clone, Default)]
pub struct BuiltinModels {
    pub general_purpose: Option<String>,
    pub explore: Option<String>,
    pub plan: Option<String>,
    pub verification: Option<String>,
    pub claude_code_guide: Option<String>,
}

impl BuiltinModels {
    /// Get the model override for a given agent name, if any.
    pub fn get(&self, agent_name: &str) -> Option<String> {
        match agent_name {
            "general-purpose" => self.general_purpose.clone(),
            "Explore" => self.explore.clone(),
            "Plan" => self.plan.clone(),
            "Verification" => self.verification.clone(),
            "claude-code-guide" => self.claude_code_guide.clone(),
            _ => None,
        }
    }
}

/// Create all 5 builtin agents with model overrides applied.
/// Model priority: builtin_models override → code default → None (inherit)
pub fn builtin_agents(builtin_models: &BuiltinModels) -> Vec<AgentDefinition> {
    vec![
        general_purpose_agent(builtin_models.get("general-purpose")),
        explore_agent(builtin_models.get("Explore")),
        plan_agent(builtin_models.get("Plan")),
        verification_agent(builtin_models.get("Verification")),
        claude_code_guide_agent(builtin_models.get("claude-code-guide")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_agents_count() {
        let models = BuiltinModels::default();
        let agents = builtin_agents(&models);
        assert_eq!(agents.len(), 5);
    }

    #[test]
    fn test_builtin_agent_names() {
        let models = BuiltinModels::default();
        let agents = builtin_agents(&models);
        let names: Vec<&str> = agents.iter().map(|a| a.name.as_str()).collect();

        assert!(names.contains(&"general-purpose"));
        assert!(names.contains(&"Explore"));
        assert!(names.contains(&"Plan"));
        assert!(names.contains(&"Verification"));
        assert!(names.contains(&"claude-code-guide"));
    }

    #[test]
    fn test_explore_has_disallowed_tools() {
        let models = BuiltinModels::default();
        let agents = builtin_agents(&models);
        let explore = agents.iter().find(|a| a.name == "Explore").unwrap();

        assert!(explore.disallowed_tools.contains(&"Edit".to_string()));
        assert!(explore.disallowed_tools.contains(&"Write".to_string()));
    }

    #[test]
    fn test_explore_default_model() {
        let models = BuiltinModels::default();
        let agents = builtin_agents(&models);
        let explore = agents.iter().find(|a| a.name == "Explore").unwrap();

        assert_eq!(explore.model, Some("haiku".to_string()));
    }

    #[test]
    fn test_plan_default_model() {
        let models = BuiltinModels::default();
        let agents = builtin_agents(&models);
        let plan = agents.iter().find(|a| a.name == "Plan").unwrap();

        assert_eq!(plan.model, None); // Inherit from parent
    }

    #[test]
    fn test_model_override() {
        let mut models = BuiltinModels::default();
        models.plan = Some("sonnet".to_string());

        let agents = builtin_agents(&models);
        let plan = agents.iter().find(|a| a.name == "Plan").unwrap();

        assert_eq!(plan.model, Some("sonnet".to_string()));
    }

    #[test]
    fn test_verification_has_worktree_isolation() {
        let models = BuiltinModels::default();
        let agents = builtin_agents(&models);
        let verification = agents.iter().find(|a| a.name == "Verification").unwrap();

        assert_eq!(verification.isolation, Some("worktree".to_string()));
    }

    #[test]
    fn test_claude_code_guide_tool_filter() {
        let models = BuiltinModels::default();
        let agents = builtin_agents(&models);
        let guide = agents.iter().find(|a| a.name == "claude-code-guide").unwrap();

        match &guide.tool_filter {
            AgentToolFilter::AllowList(tools) => {
                assert!(tools.contains(&"WebFetch".to_string()));
                assert!(tools.contains(&"WebSearch".to_string()));
                assert!(!tools.contains(&"Edit".to_string()));
            }
            _ => panic!("Expected AllowList"),
        }
    }
}
