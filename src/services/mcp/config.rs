use std::collections::HashMap;
use std::path::Path;

use super::types::McpServerConfig;

/// MCP config file format (matches .mcp.json)
#[derive(serde::Serialize, serde::Deserialize)]
pub struct McpJsonConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

/// Load MCP server configs from .mcp.json files.
/// Discovery order (later overrides earlier):
/// 1. ~/.atom/.mcp.json (user-level)
/// 2. CWD/.mcp.json (project-level)
/// 3. --mcp-config CLI paths
pub fn load_mcp_configs(cwd: &Path, extra_paths: &[String]) -> HashMap<String, McpServerConfig> {
    let mut configs = HashMap::new();

    // 1. User-level
    {
        let user_config = crate::utils::paths::user_mcp_config_path();
        if let Some(parsed) = parse_mcp_json(&user_config) {
            configs.extend(parsed);
        }
    }

    // 2. Project-level (CWD)
    let project_config = cwd.join(".mcp.json");
    if let Some(parsed) = parse_mcp_json(&project_config) {
        configs.extend(parsed); // project overrides user
    }

    // 3. Extra --mcp-config paths
    for path in extra_paths {
        if path.starts_with('{') {
            // Inline JSON string
            if let Ok(parsed) = serde_json::from_str::<McpJsonConfig>(path) {
                configs.extend(parsed.mcp_servers);
            }
        } else {
            let p = std::path::Path::new(path);
            if let Some(parsed) = parse_mcp_json(p) {
                configs.extend(parsed);
            }
        }
    }

    configs
}

pub fn parse_mcp_json(path: &Path) -> Option<HashMap<String, McpServerConfig>> {
    let content = std::fs::read_to_string(path).ok()?;
    let config: McpJsonConfig = serde_json::from_str(&content).ok()?;
    Some(config.mcp_servers)
}

/// Read MCP config from .mcp.json in the given directory, returning empty map if missing.
pub fn read_mcp_json(dir: &Path) -> HashMap<String, McpServerConfig> {
    let path = dir.join(".mcp.json");
    parse_mcp_json(&path).unwrap_or_default()
}

/// Write MCP config to .mcp.json in the given directory.
pub fn write_mcp_json(dir: &Path, configs: &HashMap<String, McpServerConfig>) -> anyhow::Result<()> {
    let path = dir.join(".mcp.json");
    let config = McpJsonConfig {
        mcp_servers: configs.clone(),
    };
    let json = serde_json::to_string_pretty(&config)?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_parse_mcp_json() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".mcp.json");
        let mut f = std::fs::File::create(&config_path).unwrap();
        write!(
            f,
            r#"{{
                "mcpServers": {{
                    "filesystem": {{
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
                    }}
                }}
            }}"#
        )
        .unwrap();

        let result = parse_mcp_json(&config_path).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result.contains_key("filesystem"));
        let fs_config = &result["filesystem"];
        assert_eq!(fs_config.command, "npx");
        assert_eq!(fs_config.args.len(), 3);
    }

    #[test]
    fn test_parse_mcp_json_missing_file() {
        let result = parse_mcp_json(Path::new("/nonexistent/.mcp.json"));
        assert!(result.is_none());
    }

    #[test]
    fn test_load_mcp_configs_from_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".mcp.json");
        let mut f = std::fs::File::create(&config_path).unwrap();
        write!(
            f,
            r#"{{
                "mcpServers": {{
                    "test-server": {{
                        "command": "echo",
                        "args": ["hello"]
                    }}
                }}
            }}"#
        )
        .unwrap();

        let configs = load_mcp_configs(dir.path(), &[]);
        assert!(configs.contains_key("test-server"));
    }

    #[test]
    fn test_load_mcp_configs_inline_json() {
        let dir = tempfile::tempdir().unwrap();
        let inline = r#"{"mcpServers":{"inline":{"command":"test","args":[]}}}"#.to_string();

        let configs = load_mcp_configs(dir.path(), &[inline]);
        assert!(configs.contains_key("inline"));
    }

    #[test]
    fn test_load_mcp_configs_project_overrides_user() {
        // Project-level config should override user-level for same key.
        // We test just the project part since user config path is system-dependent.
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".mcp.json");
        let mut f = std::fs::File::create(&config_path).unwrap();
        write!(
            f,
            r#"{{
                "mcpServers": {{
                    "server": {{
                        "command": "project-cmd",
                        "args": []
                    }}
                }}
            }}"#
        )
        .unwrap();

        let configs = load_mcp_configs(dir.path(), &[]);
        assert_eq!(configs["server"].command, "project-cmd");
    }

    #[test]
    fn test_load_mcp_configs_with_env() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".mcp.json");
        let mut f = std::fs::File::create(&config_path).unwrap();
        write!(
            f,
            r#"{{
                "mcpServers": {{
                    "with-env": {{
                        "command": "server",
                        "args": [],
                        "env": {{
                            "API_KEY": "secret123"
                        }}
                    }}
                }}
            }}"#
        )
        .unwrap();

        let configs = load_mcp_configs(dir.path(), &[]);
        let env = &configs["with-env"].env;
        assert_eq!(env.get("API_KEY").unwrap(), "secret123");
    }
}
