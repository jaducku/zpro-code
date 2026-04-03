use std::path::PathBuf;

/// Global config directory: ~/.atom
/// All user-level configuration, sessions, skills, hooks, MCP configs live here.
pub fn global_config_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    PathBuf::from(home).join(".atom")
}

/// Sessions directory: ~/.atom/sessions/
pub fn sessions_dir() -> PathBuf {
    global_config_dir().join("sessions")
}

/// User-level skills directory: ~/.atom/skills/
pub fn user_skills_dir() -> PathBuf {
    global_config_dir().join("skills")
}

/// User-level hooks config: ~/.atom/hooks.json
pub fn user_hooks_path() -> PathBuf {
    global_config_dir().join("hooks.json")
}

/// User-level MCP config: ~/.atom/.mcp.json
pub fn user_mcp_config_path() -> PathBuf {
    global_config_dir().join(".mcp.json")
}

/// User-level AGENTS.md: ~/.atom/AGENTS.md
pub fn user_agents_md_path() -> PathBuf {
    global_config_dir().join("AGENTS.md")
}

/// Base directory for per-project memory: ~/.atom/projects/
pub fn user_memory_base_dir() -> PathBuf {
    global_config_dir().join("projects")
}

/// Project-level config directory name
pub const PROJECT_CONFIG_DIR: &str = ".agents";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_global_config_dir() {
        let dir = global_config_dir();
        assert!(dir.to_string_lossy().contains(".atom"));
    }

    #[test]
    fn test_sessions_dir() {
        let dir = sessions_dir();
        assert!(dir.to_string_lossy().ends_with(".atom/sessions"));
    }
}
