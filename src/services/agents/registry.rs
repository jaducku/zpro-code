use super::types::AgentDefinition;
use std::collections::HashMap;

/// Registry of all available agents with override precedence.
/// Higher-priority sources (ProjectSettings > UserSettings > Plugin > BuiltIn)
/// override lower-priority definitions with the same name.
#[derive(Debug, Clone)]
pub struct AgentRegistry {
    agents: HashMap<String, AgentDefinition>,
}

impl AgentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    /// Create a registry with builtin agents pre-populated.
    /// Call this to initialize before adding other sources.
    pub fn with_builtins(builtins: Vec<AgentDefinition>) -> Self {
        let mut registry = Self::new();
        for agent in builtins {
            registry.agents.insert(agent.name.clone(), agent);
        }
        registry
    }

    /// Register an agent definition, with override logic based on source precedence.
    /// If an agent with the same name exists:
    /// - Higher AgentSource value (ProjectSettings > UserSettings > Plugin > BuiltIn) wins
    /// - Otherwise, the new definition overwrites
    pub fn register(&mut self, agent: AgentDefinition) {
        let name = agent.name.clone();
        match self.agents.get(&name) {
            Some(existing) if existing.source >= agent.source => {
                // Existing has equal or higher priority, keep it
            }
            _ => {
                // New agent has higher priority or name doesn't exist, insert/update
                self.agents.insert(name, agent);
            }
        }
    }

    /// Register multiple agents at once.
    pub fn register_all(&mut self, agents: Vec<AgentDefinition>) {
        for agent in agents {
            self.register(agent);
        }
    }

    /// Get an agent by name.
    pub fn get(&self, name: &str) -> Option<&AgentDefinition> {
        self.agents.get(name)
    }

    /// Get all agents.
    pub fn all(&self) -> Vec<&AgentDefinition> {
        self.agents.values().collect()
    }

    /// Check if an agent is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }

    /// Get agent count.
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::agents::types::{AgentSource, AgentToolFilter};

    fn agent(name: &str, source: AgentSource) -> AgentDefinition {
        AgentDefinition {
            name: name.to_string(),
            description: "test".to_string(),
            source,
            system_prompt: None,
            tool_filter: AgentToolFilter::AllTools,
            disallowed_tools: Vec::new(),
            model: None,
            max_turns: None,
            permission_mode: None,
            isolation: None,
            query_source: format!("agent:test:{}", name),
        }
    }

    #[test]
    fn test_register_new_agent() {
        let mut registry = AgentRegistry::new();
        let ag = agent("test-agent", AgentSource::BuiltIn);
        registry.register(ag.clone());

        assert!(registry.contains("test-agent"));
        assert_eq!(
            registry.get("test-agent").map(|a| &a.name),
            Some(&"test-agent".to_string())
        );
    }

    #[test]
    fn test_override_precedence() {
        let mut registry = AgentRegistry::new();

        // Register builtin
        let builtin = agent("agent", AgentSource::BuiltIn);
        registry.register(builtin);

        // Try to override with plugin (higher priority)
        let plugin = agent("agent", AgentSource::Plugin);
        registry.register(plugin);
        assert_eq!(
            registry.get("agent").map(|a| a.source),
            Some(AgentSource::Plugin)
        );

        // Try to downgrade to user (still higher than plugin) - should win
        let user = agent("agent", AgentSource::UserSettings);
        registry.register(user);
        assert_eq!(
            registry.get("agent").map(|a| a.source),
            Some(AgentSource::UserSettings)
        );

        // Project overrides everything
        let project = agent("agent", AgentSource::ProjectSettings);
        registry.register(project);
        assert_eq!(
            registry.get("agent").map(|a| a.source),
            Some(AgentSource::ProjectSettings)
        );

        // Try to downgrade from project - should keep project
        let builtin2 = agent("agent", AgentSource::BuiltIn);
        registry.register(builtin2);
        assert_eq!(
            registry.get("agent").map(|a| a.source),
            Some(AgentSource::ProjectSettings)
        );
    }

    #[test]
    fn test_register_all() {
        let mut registry = AgentRegistry::new();
        let agents = vec![
            agent("agent1", AgentSource::BuiltIn),
            agent("agent2", AgentSource::Plugin),
            agent("agent3", AgentSource::UserSettings),
        ];

        registry.register_all(agents);
        assert_eq!(registry.len(), 3);
        assert!(registry.contains("agent1"));
        assert!(registry.contains("agent2"));
        assert!(registry.contains("agent3"));
    }

    #[test]
    fn test_with_builtins() {
        let builtins = vec![
            agent("builtin1", AgentSource::BuiltIn),
            agent("builtin2", AgentSource::BuiltIn),
        ];

        let registry = AgentRegistry::with_builtins(builtins);
        assert_eq!(registry.len(), 2);
        assert!(registry.contains("builtin1"));
        assert!(registry.contains("builtin2"));
    }
}
