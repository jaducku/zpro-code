use super::types::{AgentDefinition, AgentSource, AgentToolFilter};
use super::builtins::{builtin_agents, BuiltinModels};
use super::registry::AgentRegistry;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

/// Parse agent definition from Markdown frontmatter + body.
/// Format:
/// ---
/// name: agent-name
/// description: "What this agent does"
/// model: sonnet
/// tools: [Tool1, Tool2]  or ["*"]  or omit for AllTools
/// disallowedTools: [Tool3, Tool4]
/// maxTurns: 30
/// permissionMode: default
/// isolation: worktree
/// ---
/// System prompt content here...
pub fn parse_agent_md(name: &str, content: &str) -> Result<AgentDefinition> {
    let (frontmatter, body) = split_frontmatter(content);

    // Parse YAML frontmatter
    let yaml: serde_yaml::Value = serde_yaml::from_str(&frontmatter)
        .with_context(|| format!("failed to parse YAML for agent '{name}'"))?;

    let description = yaml
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("No description provided")
        .to_string();

    // Parse tools field (supports both camelCase and snake_case)
    let tool_filter = parse_tool_filter(&yaml)?;

    // Parse disallowed tools (supports both camelCase and snake_case)
    let disallowed_tools = parse_disallowed_tools(&yaml);

    // Parse other optional fields
    let model = yaml
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let max_turns = yaml
        .get("maxTurns")
        .or_else(|| yaml.get("max_turns"))
        .and_then(|v| v.as_u64())
        .map(|u| u as u32);

    let permission_mode = yaml
        .get("permissionMode")
        .or_else(|| yaml.get("permission_mode"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let isolation = yaml
        .get("isolation")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Body becomes system_prompt (None if empty)
    let system_prompt = if body.trim().is_empty() {
        None
    } else {
        Some(body)
    };

    Ok(AgentDefinition {
        name: name.to_string(),
        description,
        source: AgentSource::UserSettings, // Will be overridden when registered
        system_prompt,
        tool_filter,
        disallowed_tools,
        model,
        max_turns,
        permission_mode,
        isolation,
        query_source: format!("agent:user:{}", name),
    })
}

/// Split markdown frontmatter from body.
/// Returns (frontmatter_yaml, body).
fn split_frontmatter(content: &str) -> (String, String) {
    let content = content.trim();
    if !content.starts_with("---") {
        return (String::new(), content.to_string());
    }

    let after_first = &content[3..];
    match after_first.find("---") {
        Some(end) => {
            let frontmatter = after_first[..end].trim().to_string();
            let body = after_first[end + 3..].trim().to_string();
            (frontmatter, body)
        }
        None => (String::new(), content.to_string()),
    }
}

/// Parse tool filter from YAML tools field.
/// - Absent or null: AllTools
/// - ["*"]: AllTools
/// - ["Tool1", "Tool2"]: AllowList
fn parse_tool_filter(yaml: &serde_yaml::Value) -> Result<AgentToolFilter> {
    let tools_value = yaml.get("tools");

    match tools_value {
        None | Some(serde_yaml::Value::Null) => Ok(AgentToolFilter::AllTools),
        Some(serde_yaml::Value::Sequence(seq)) => {
            let tools: Vec<String> = seq
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();

            // If single element is "*", treat as AllTools
            if tools.len() == 1 && tools[0] == "*" {
                Ok(AgentToolFilter::AllTools)
            } else if tools.is_empty() {
                Ok(AgentToolFilter::AllTools)
            } else {
                Ok(AgentToolFilter::AllowList(tools))
            }
        }
        _ => Err(anyhow::anyhow!("tools field must be an array or omitted")),
    }
}

/// Parse disallowed tools from YAML field (supports camelCase and snake_case).
fn parse_disallowed_tools(yaml: &serde_yaml::Value) -> Vec<String> {
    let value = yaml
        .get("disallowedTools")
        .or_else(|| yaml.get("disallowed_tools"));

    match value {
        Some(serde_yaml::Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Scan a directory for agent MD files and parse them.
pub fn scan_dir(dir: &Path, source: AgentSource) -> Result<Vec<AgentDefinition>> {
    let mut agents = Vec::new();

    if !dir.exists() {
        return Ok(agents); // Directory doesn't exist yet, return empty
    }

    for entry in std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map_or(false, |ext| ext == "md") {
            let file_stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");

            match std::fs::read_to_string(&path) {
                Ok(content) => match parse_agent_md(file_stem, &content) {
                    Ok(mut agent) => {
                        agent.source = source;
                        agent.query_source = format!(
                            "agent:{}:{}",
                            match source {
                                AgentSource::BuiltIn => "builtin",
                                AgentSource::Plugin => "plugin",
                                AgentSource::UserSettings => "user",
                                AgentSource::ProjectSettings => "project",
                            },
                            file_stem
                        );
                        agents.push(agent);
                    }
                    Err(e) => {
                        eprintln!("warning: failed to parse agent '{}': {}", file_stem, e);
                    }
                },
                Err(e) => {
                    eprintln!("warning: failed to read agent file '{}': {}", path.display(), e);
                }
            }
        }
    }

    Ok(agents)
}

/// Load all agents from all sources (builtin + files) with proper override precedence.
pub fn load_agents(
    cwd: &Path,
    plugin_paths: &[PathBuf],
    builtin_models: &BuiltinModels,
) -> Result<AgentRegistry> {
    // 1. Start with builtin agents (lowest priority)
    let mut registry = AgentRegistry::with_builtins(builtin_agents(builtin_models));

    // 2. Load plugin agents
    for plugin_path in plugin_paths {
        let agents_dir = plugin_path.join("agents");
        if let Ok(agents) = scan_dir(&agents_dir, AgentSource::Plugin) {
            for agent in agents {
                registry.register(agent);
            }
        }
    }

    // 3. Load user agents (~/.zpro-code/agents/)
    let user_agents_dir = user_agents_dir();
    if let Ok(agents) = scan_dir(&user_agents_dir, AgentSource::UserSettings) {
        for agent in agents {
            registry.register(agent);
        }
    }

    // 4. Load project agents (.agents/agents/, searching up to git root)
    let project_agents_dir = find_project_agents_dir(cwd);
    if let Ok(agents) = scan_dir(&project_agents_dir, AgentSource::ProjectSettings) {
        for agent in agents {
            registry.register(agent);
        }
    }

    Ok(registry)
}

/// Get the user agents directory (~/.zpro-code/agents/).
/// This should match user_agents_dir() in paths.rs.
fn user_agents_dir() -> PathBuf {
    crate::utils::paths::user_agents_dir()
}

/// Find project agents directory by searching from cwd up to git root.
/// Looks for .agents/agents/ subdirectory.
fn find_project_agents_dir(cwd: &Path) -> PathBuf {
    let mut current = cwd.to_path_buf();

    loop {
        let agents_dir = current.join(".agents").join("agents");
        if agents_dir.exists() {
            return agents_dir;
        }

        // Check if we've reached git root
        if current.join(".git").exists() {
            // Continue checking parent
        }

        // Try parent directory
        match current.parent() {
            Some(parent) => {
                current = parent.to_path_buf();
            }
            None => {
                // Reached filesystem root without finding .agents/agents/
                // Return default path (will be empty/non-existent)
                return cwd.join(".agents").join("agents");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml_value(yaml_str: &str) -> serde_yaml::Value {
        serde_yaml::from_str(yaml_str).unwrap()
    }

    #[test]
    fn test_parse_agent_md_basic() {
        let content = r#"---
name: test-agent
description: "A test agent"
---
This is the system prompt."#;

        let agent = parse_agent_md("test-agent", content).unwrap();
        assert_eq!(agent.name, "test-agent");
        assert_eq!(agent.description, "A test agent");
        assert_eq!(agent.system_prompt.unwrap(), "This is the system prompt.");
    }

    #[test]
    fn test_parse_agent_md_with_model() {
        let content = r#"---
description: "Test"
model: sonnet
---
Prompt"#;

        let agent = parse_agent_md("test", content).unwrap();
        assert_eq!(agent.model, Some("sonnet".to_string()));
    }

    #[test]
    fn test_parse_tool_filter_all_tools() {
        let yaml = yaml_value("{}");
        let filter = parse_tool_filter(&yaml).unwrap();
        match filter {
            AgentToolFilter::AllTools => (),
            _ => panic!("Expected AllTools"),
        }
    }

    #[test]
    fn test_parse_tool_filter_wildcard() {
        let yaml = yaml_value("tools: ['*']");
        let filter = parse_tool_filter(&yaml).unwrap();
        match filter {
            AgentToolFilter::AllTools => (),
            _ => panic!("Expected AllTools"),
        }
    }

    #[test]
    fn test_parse_tool_filter_allowlist() {
        let yaml = yaml_value("tools: [Read, Grep, Edit]");
        let filter = parse_tool_filter(&yaml).unwrap();
        match filter {
            AgentToolFilter::AllowList(tools) => {
                assert_eq!(tools.len(), 3);
                assert!(tools.contains(&"Read".to_string()));
            }
            _ => panic!("Expected AllowList"),
        }
    }

    #[test]
    fn test_split_frontmatter_with_content() {
        let content = "---\nkey: value\n---\nBody content here";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.contains("key: value"));
        assert_eq!(body, "Body content here");
    }

    #[test]
    fn test_split_frontmatter_no_frontmatter() {
        let content = "Just body content";
        let (fm, body) = split_frontmatter(content);
        assert!(fm.is_empty());
        assert_eq!(body, "Just body content");
    }
}
