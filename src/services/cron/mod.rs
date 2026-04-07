use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

use crate::utils::paths::global_config_dir;

const MAX_TASKS: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronTask {
    pub id: String,
    pub cron: String,
    pub prompt: String,
    pub recurring: bool,
    pub durable: bool,
    pub created_at: DateTime<Utc>,
    pub last_fired_at: Option<DateTime<Utc>>,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DurableFile {
    tasks: Vec<CronTask>,
}

#[derive(Debug, Clone)]
pub struct CronStore {
    session_tasks: Vec<CronTask>,
    durable_path: PathBuf,
}

impl CronStore {
    pub fn new() -> Self {
        Self {
            session_tasks: Vec::new(),
            durable_path: global_config_dir().join("scheduled_tasks.json"),
        }
    }

    pub fn add(&mut self, task: CronTask) -> Result<(), String> {
        let total = self.session_tasks.len() + self.load_durable().len();
        if total >= MAX_TASKS {
            return Err(format!("Maximum of {} scheduled tasks reached", MAX_TASKS));
        }

        if task.durable {
            let mut durable = self.load_durable();
            durable.push(task);
            self.save_durable(&durable)?;
        } else {
            self.session_tasks.push(task);
        }

        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<CronTask, String> {
        // Check session tasks first
        if let Some(pos) = self.session_tasks.iter().position(|t| t.id == id) {
            return Ok(self.session_tasks.remove(pos));
        }

        // Check durable tasks
        let mut durable = self.load_durable();
        if let Some(pos) = durable.iter().position(|t| t.id == id) {
            let removed = durable.remove(pos);
            self.save_durable(&durable)?;
            return Ok(removed);
        }

        Err(format!("Task '{}' not found", id))
    }

    pub fn list(&self, agent_id_filter: Option<&str>) -> Vec<CronTask> {
        let mut all: Vec<CronTask> = self.session_tasks.clone();
        all.extend(self.load_durable());

        if let Some(agent_id) = agent_id_filter {
            all.retain(|t| t.agent_id.as_deref() == Some(agent_id));
        }

        all
    }

    fn load_durable(&self) -> Vec<CronTask> {
        let Ok(data) = std::fs::read_to_string(&self.durable_path) else {
            return Vec::new();
        };
        let Ok(file) = serde_json::from_str::<DurableFile>(&data) else {
            return Vec::new();
        };
        file.tasks
    }

    fn save_durable(&self, tasks: &[CronTask]) -> Result<(), String> {
        let file = DurableFile {
            tasks: tasks.to_vec(),
        };
        let json = serde_json::to_string_pretty(&file)
            .map_err(|e| format!("Failed to serialize durable tasks: {}", e))?;

        if let Some(parent) = self.durable_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }

        std::fs::write(&self.durable_path, json)
            .map_err(|e| format!("Failed to write durable tasks: {}", e))?;
        Ok(())
    }
}

impl Default for CronStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a cron expression. Accepts 5-field (minute-level) or 6-field (second-level).
/// For 5-field expressions, prepends "0 " to make it 6-field for the cron crate.
pub fn validate_cron(expr: &str) -> Result<String, String> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    let normalized = match fields.len() {
        5 => format!("0 {}", expr),
        6 => expr.to_string(),
        _ => {
            return Err(format!(
                "Invalid cron expression '{}': expected 5 or 6 fields",
                expr
            ))
        }
    };

    cron::Schedule::from_str(&normalized)
        .map_err(|e| format!("Invalid cron expression '{}': {}", expr, e))?;

    Ok(normalized)
}

/// Produce a human-readable description of a cron expression.
pub fn human_schedule(expr: &str) -> String {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    // Work with the 5-field form (min hour dom month dow)
    let (min, hour, dom, month, dow) = match fields.len() {
        5 => (fields[0], fields[1], fields[2], fields[3], fields[4]),
        6 => (fields[1], fields[2], fields[3], fields[4], fields[5]),
        _ => return format!("cron: {}", expr),
    };

    let mut parts = Vec::new();

    // Minute
    if min == "*" {
        parts.push("every minute".to_string());
    } else if let Some(step) = min.strip_prefix("*/") {
        parts.push(format!("every {} minutes", step));
    } else {
        parts.push(format!("at minute {}", min));
    }

    // Hour
    if hour != "*" {
        if let Some(step) = hour.strip_prefix("*/") {
            parts.push(format!("every {} hours", step));
        } else {
            parts.push(format!("hour {}", hour));
        }
    }

    // Day of month
    if dom != "*" {
        parts.push(format!("on day {}", dom));
    }

    // Month
    if month != "*" {
        parts.push(format!("in month {}", month));
    }

    // Day of week
    if dow != "*" {
        let day_name = match dow {
            "0" | "7" => "Sunday",
            "1" => "Monday",
            "2" => "Tuesday",
            "3" => "Wednesday",
            "4" => "Thursday",
            "5" => "Friday",
            "6" => "Saturday",
            _ => dow,
        };
        parts.push(format!("on {}", day_name));
    }

    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_cron_valid_5_field() {
        let result = validate_cron("*/5 * * * *");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "0 */5 * * * *");
    }

    #[test]
    fn test_validate_cron_valid_6_field() {
        let result = validate_cron("0 30 9 * * *");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_cron_invalid() {
        let result = validate_cron("not a cron");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid cron expression"));
    }

    #[test]
    fn test_validate_cron_wrong_field_count() {
        let result = validate_cron("* * *");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expected 5 or 6 fields"));
    }

    #[test]
    fn test_human_schedule_every_5_min() {
        let desc = human_schedule("*/5 * * * *");
        assert!(desc.contains("every 5 minutes"));
    }

    #[test]
    fn test_human_schedule_specific_hour() {
        let desc = human_schedule("30 9 * * *");
        assert!(desc.contains("minute 30"));
        assert!(desc.contains("hour 9"));
    }

    #[test]
    fn test_human_schedule_with_dow() {
        let desc = human_schedule("0 8 * * 1");
        assert!(desc.contains("Monday"));
    }

    #[test]
    fn test_add_remove_session_tasks() {
        let mut store = CronStore {
            session_tasks: Vec::new(),
            durable_path: PathBuf::from("/tmp/nonexistent_cron_test.json"),
        };

        let task = CronTask {
            id: "test-1".into(),
            cron: "0 */5 * * * *".into(),
            prompt: "do stuff".into(),
            recurring: true,
            durable: false,
            created_at: Utc::now(),
            last_fired_at: None,
            agent_id: None,
        };

        store.add(task.clone()).unwrap();
        assert_eq!(store.list(None).len(), 1);

        let removed = store.remove("test-1").unwrap();
        assert_eq!(removed.id, "test-1");
        assert!(store.list(None).is_empty());
    }

    #[test]
    fn test_add_durable_task() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scheduled_tasks.json");

        let mut store = CronStore {
            session_tasks: Vec::new(),
            durable_path: path.clone(),
        };

        let task = CronTask {
            id: "durable-1".into(),
            cron: "0 0 * * * *".into(),
            prompt: "hourly check".into(),
            recurring: true,
            durable: true,
            created_at: Utc::now(),
            last_fired_at: None,
            agent_id: None,
        };

        store.add(task).unwrap();

        // Verify file was written
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        let file: DurableFile = serde_json::from_str(&content).unwrap();
        assert_eq!(file.tasks.len(), 1);
        assert_eq!(file.tasks[0].id, "durable-1");
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut store = CronStore {
            session_tasks: Vec::new(),
            durable_path: PathBuf::from("/tmp/nonexistent_cron_test.json"),
        };

        let result = store.remove("does-not-exist");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_max_tasks_limit() {
        let mut store = CronStore {
            session_tasks: Vec::new(),
            durable_path: PathBuf::from("/tmp/nonexistent_cron_test.json"),
        };

        for i in 0..MAX_TASKS {
            let task = CronTask {
                id: format!("task-{}", i),
                cron: "0 * * * * *".into(),
                prompt: "test".into(),
                recurring: true,
                durable: false,
                created_at: Utc::now(),
                last_fired_at: None,
                agent_id: None,
            };
            store.add(task).unwrap();
        }

        let extra = CronTask {
            id: "task-overflow".into(),
            cron: "0 * * * * *".into(),
            prompt: "too many".into(),
            recurring: true,
            durable: false,
            created_at: Utc::now(),
            last_fired_at: None,
            agent_id: None,
        };

        let result = store.add(extra);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Maximum"));
    }

    #[test]
    fn test_list_with_agent_filter() {
        let mut store = CronStore {
            session_tasks: Vec::new(),
            durable_path: PathBuf::from("/tmp/nonexistent_cron_test.json"),
        };

        store
            .add(CronTask {
                id: "a1".into(),
                cron: "0 * * * * *".into(),
                prompt: "test".into(),
                recurring: true,
                durable: false,
                created_at: Utc::now(),
                last_fired_at: None,
                agent_id: Some("agent-1".into()),
            })
            .unwrap();

        store
            .add(CronTask {
                id: "a2".into(),
                cron: "0 * * * * *".into(),
                prompt: "test".into(),
                recurring: true,
                durable: false,
                created_at: Utc::now(),
                last_fired_at: None,
                agent_id: Some("agent-2".into()),
            })
            .unwrap();

        assert_eq!(store.list(None).len(), 2);
        assert_eq!(store.list(Some("agent-1")).len(), 1);
        assert_eq!(store.list(Some("agent-3")).len(), 0);
    }
}
