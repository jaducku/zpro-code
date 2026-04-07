use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub lead_agent_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lead_session_id: Option<String>,
    #[serde(default)]
    pub members: Vec<TeamMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamMember {
    pub agent_id: String,
    pub name: String,
    pub joined_at: DateTime<Utc>,
    #[serde(default)]
    pub is_active: bool,
}

/// Sanitize a team name for use in file paths (same as TS: replace non-alnum with `-`, lowercase).
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

pub struct TeamService {
    base_dir: PathBuf,
    tasks_dir: PathBuf,
}

impl Default for TeamService {
    fn default() -> Self {
        Self::new()
    }
}

impl TeamService {
    pub fn new() -> Self {
        let base = crate::utils::paths::global_config_dir();
        Self {
            base_dir: base.join("teams"),
            tasks_dir: base.join("tasks"),
        }
    }

    #[cfg(test)]
    fn with_base(base_dir: PathBuf, tasks_dir: PathBuf) -> Self {
        Self {
            base_dir,
            tasks_dir,
        }
    }

    fn team_dir(&self, name: &str) -> PathBuf {
        self.base_dir.join(sanitize_name(name))
    }

    fn config_path(&self, name: &str) -> PathBuf {
        self.team_dir(name).join("config.json")
    }

    fn task_dir(&self, name: &str) -> PathBuf {
        self.tasks_dir.join(sanitize_name(name))
    }

    /// Create a new team. Returns the config and its file path.
    pub fn create(
        &self,
        name: &str,
        description: Option<&str>,
        session_id: Option<&str>,
    ) -> anyhow::Result<(TeamConfig, PathBuf)> {
        let team_dir = self.team_dir(name);
        std::fs::create_dir_all(&team_dir)?;

        let task_dir = self.task_dir(name);
        std::fs::create_dir_all(&task_dir)?;

        let lead_agent_id = format!("team-lead@{}", sanitize_name(name));

        let config = TeamConfig {
            name: name.to_string(),
            description: description.map(String::from),
            created_at: Utc::now(),
            lead_agent_id: lead_agent_id.clone(),
            lead_session_id: session_id.map(String::from),
            members: vec![TeamMember {
                agent_id: lead_agent_id,
                name: "team-lead".to_string(),
                joined_at: Utc::now(),
                is_active: true,
            }],
        };

        let config_path = self.config_path(name);
        let json = serde_json::to_string_pretty(&config)?;
        std::fs::write(&config_path, json)?;

        Ok((config, config_path))
    }

    /// Load a team config by name. Returns None if not found.
    pub fn load(&self, name: &str) -> anyhow::Result<Option<TeamConfig>> {
        let path = self.config_path(name);
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(Some(serde_json::from_str(&content)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete a team. Errors if there are active non-lead members.
    pub fn delete(&self, name: &str) -> anyhow::Result<()> {
        if let Some(config) = self.load(name)? {
            let active = active_member_count(&config);
            if active > 0 {
                anyhow::bail!(
                    "Cannot delete team with {} active member(s). Use requestShutdown to terminate teammates first.",
                    active
                );
            }
        }

        let team_dir = self.team_dir(name);
        if team_dir.exists() {
            std::fs::remove_dir_all(&team_dir)?;
        }

        let task_dir = self.task_dir(name);
        if task_dir.exists() {
            std::fs::remove_dir_all(&task_dir)?;
        }

        Ok(())
    }
}

/// Count active members excluding the team lead.
pub fn active_member_count(config: &TeamConfig) -> usize {
    config
        .members
        .iter()
        .filter(|m| m.is_active && m.agent_id != config.lead_agent_id)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> (TeamService, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let svc = TeamService::with_base(dir.path().join("teams"), dir.path().join("tasks"));
        (svc, dir)
    }

    #[test]
    fn test_create_and_load() {
        let (svc, _dir) = test_service();
        let (config, path) = svc
            .create("my-team", Some("A test team"), Some("sess-1"))
            .unwrap();

        assert_eq!(config.name, "my-team");
        assert_eq!(config.description.as_deref(), Some("A test team"));
        assert_eq!(config.lead_agent_id, "team-lead@my-team");
        assert_eq!(config.lead_session_id.as_deref(), Some("sess-1"));
        assert_eq!(config.members.len(), 1);
        assert!(path.ends_with("teams/my-team/config.json"));

        let loaded = svc.load("my-team").unwrap().unwrap();
        assert_eq!(loaded.name, "my-team");
        assert_eq!(loaded.lead_agent_id, "team-lead@my-team");
    }

    #[test]
    fn test_delete() {
        let (svc, _dir) = test_service();
        svc.create("del-team", None, None).unwrap();

        assert!(svc.load("del-team").unwrap().is_some());
        svc.delete("del-team").unwrap();
        assert!(svc.load("del-team").unwrap().is_none());
    }

    #[test]
    fn test_delete_with_active_members() {
        let (svc, _dir) = test_service();
        let (mut config, _) = svc.create("busy-team", None, None).unwrap();

        // Add an active non-lead member
        config.members.push(TeamMember {
            agent_id: "worker-1".to_string(),
            name: "worker".to_string(),
            joined_at: Utc::now(),
            is_active: true,
        });
        // Write back
        let json = serde_json::to_string_pretty(&config).unwrap();
        std::fs::write(svc.config_path("busy-team"), json).unwrap();

        let err = svc.delete("busy-team").unwrap_err();
        assert!(err.to_string().contains("active member"));
    }

    #[test]
    fn test_active_member_count() {
        let config = TeamConfig {
            name: "test".to_string(),
            description: None,
            created_at: Utc::now(),
            lead_agent_id: "team-lead@test".to_string(),
            lead_session_id: None,
            members: vec![
                TeamMember {
                    agent_id: "team-lead@test".to_string(),
                    name: "team-lead".to_string(),
                    joined_at: Utc::now(),
                    is_active: true,
                },
                TeamMember {
                    agent_id: "worker-1".to_string(),
                    name: "worker-1".to_string(),
                    joined_at: Utc::now(),
                    is_active: true,
                },
                TeamMember {
                    agent_id: "worker-2".to_string(),
                    name: "worker-2".to_string(),
                    joined_at: Utc::now(),
                    is_active: false,
                },
            ],
        };

        assert_eq!(active_member_count(&config), 1);
    }
}
