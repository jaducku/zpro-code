/// Source of agent definition. Used for override precedence.
/// Higher value overrides lower value (ProjectSettings > UserSettings > Plugin > BuiltIn).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgentSource {
    BuiltIn = 0,
    Plugin = 1,
    UserSettings = 2,
    ProjectSettings = 3,
}

/// Tool filtering strategy for agents.
#[derive(Debug, Clone)]
pub enum AgentToolFilter {
    /// All tools are allowed (default behavior)
    AllTools,
    /// Only explicitly listed tools are allowed
    AllowList(Vec<String>),
}

/// Complete definition of an agent with all configuration.
#[derive(Debug, Clone)]
pub struct AgentDefinition {
    /// Unique agent name (e.g., "Explore", "Plan", "code-reviewer")
    pub name: String,

    /// Description of what the agent does (whenToUse field from YAML)
    pub description: String,

    /// Source where this agent is defined (for override precedence)
    pub source: AgentSource,

    /// Custom system prompt. None means inherit from parent.
    pub system_prompt: Option<String>,

    /// Tool filtering strategy (AllTools or AllowList)
    pub tool_filter: AgentToolFilter,

    /// Tools to explicitly disallow (independent of tool_filter)
    pub disallowed_tools: Vec<String>,

    /// Model to use. None means inherit from parent.
    /// Special value "inherit" is also treated as inheritance at runtime.
    pub model: Option<String>,

    /// Maximum number of turns for this agent. None uses default (50).
    pub max_turns: Option<u32>,

    /// Permission mode override (e.g., "default", "plan", "acceptEdits")
    pub permission_mode: Option<String>,

    /// Isolation mode (e.g., "worktree")
    pub isolation: Option<String>,

    /// Query source identifier (e.g., "agent:builtin:explore")
    /// Used to track which agent spawned subagents
    pub query_source: String,
}

impl AgentDefinition {
    /// Create a new agent definition with required fields.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        source: AgentSource,
    ) -> Self {
        let name = name.into();
        Self {
            query_source: format!(
                "agent:{}:{}",
                match source {
                    AgentSource::BuiltIn => "builtin",
                    AgentSource::Plugin => "plugin",
                    AgentSource::UserSettings => "user",
                    AgentSource::ProjectSettings => "project",
                },
                name.to_lowercase()
            ),
            name,
            description: description.into(),
            source,
            system_prompt: None,
            tool_filter: AgentToolFilter::AllTools,
            disallowed_tools: Vec::new(),
            model: None,
            max_turns: None,
            permission_mode: None,
            isolation: None,
        }
    }
}
