use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub subject: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub active_form: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub blocks: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
}

pub struct TaskUpdate {
    pub subject: Option<String>,
    pub description: Option<String>,
    pub active_form: Option<String>,
    pub status: Option<TaskStatus>,
    pub owner: Option<String>,
    pub add_blocks: Option<Vec<String>>,
    pub add_blocked_by: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

pub struct TaskListStore {
    base_dir: PathBuf,
}

impl Default for TaskListStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskListStore {
    pub fn new() -> Self {
        Self {
            base_dir: crate::utils::paths::global_config_dir().join("tasks"),
        }
    }

    pub fn with_base(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn dir(&self, list_id: &str) -> PathBuf {
        self.base_dir.join(list_id)
    }

    /// Create a new task. Returns the created task.
    /// Uses file locking for concurrency. Lock held during ID allocation + file write.
    pub fn create_task(
        &self,
        list_id: &str,
        subject: &str,
        description: &str,
        active_form: Option<&str>,
        metadata: Option<HashMap<String, serde_json::Value>>,
    ) -> anyhow::Result<Task> {
        let dir = self.dir(list_id);
        std::fs::create_dir_all(&dir)?;

        // Acquire exclusive lock
        let lock_path = dir.join("_lock");
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lock_path)?;
        use fs2::FileExt;
        lock_file.lock_exclusive()?;

        // Read and increment high water mark
        let next_id_path = dir.join("_next_id");
        let next_id: u64 = std::fs::read_to_string(&next_id_path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(1);
        let id = next_id.to_string();
        std::fs::write(&next_id_path, (next_id + 1).to_string())?;

        // Create task
        let task = Task {
            id: id.clone(),
            subject: subject.to_string(),
            description: description.to_string(),
            status: TaskStatus::Pending,
            active_form: active_form.map(String::from),
            owner: None,
            blocks: Vec::new(),
            blocked_by: Vec::new(),
            metadata: metadata.unwrap_or_default(),
        };

        // Atomic write: write to .tmp, then rename
        let task_path = dir.join(format!("{}.json", id));
        let tmp_path = dir.join(format!("{}.json.tmp", id));
        let json = serde_json::to_string_pretty(&task)?;
        std::fs::write(&tmp_path, &json)?;
        std::fs::rename(&tmp_path, &task_path)?;

        // Lock released when lock_file is dropped
        Ok(task)
    }

    /// Get a single task by ID.
    pub fn get_task(&self, list_id: &str, task_id: &str) -> anyhow::Result<Option<Task>> {
        let path = self.dir(list_id).join(format!("{}.json", task_id));
        match std::fs::read_to_string(&path) {
            Ok(content) => Ok(Some(serde_json::from_str(&content)?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all tasks in a task list.
    pub fn list_tasks(&self, list_id: &str) -> anyhow::Result<Vec<Task>> {
        let dir = self.dir(list_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut tasks = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") && !name.starts_with('_') && !name.ends_with(".tmp") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(task) = serde_json::from_str::<Task>(&content) {
                        tasks.push(task);
                    }
                }
            }
        }
        tasks.sort_by(|a, b| {
            a.id.parse::<u64>()
                .unwrap_or(0)
                .cmp(&b.id.parse::<u64>().unwrap_or(0))
        });
        Ok(tasks)
    }

    /// Update a task. Returns the updated task.
    pub fn update_task(
        &self,
        list_id: &str,
        task_id: &str,
        updates: TaskUpdate,
    ) -> anyhow::Result<Task> {
        let dir = self.dir(list_id);

        // Lock
        let lock_path = dir.join("_lock");
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&lock_path)?;
        use fs2::FileExt;
        lock_file.lock_exclusive()?;

        let task_path = dir.join(format!("{}.json", task_id));
        let content = std::fs::read_to_string(&task_path)
            .map_err(|_| anyhow::anyhow!("Task {} not found", task_id))?;
        let mut task: Task = serde_json::from_str(&content)?;

        // Apply updates
        if let Some(s) = updates.subject {
            task.subject = s;
        }
        if let Some(d) = updates.description {
            task.description = d;
        }
        if let Some(a) = updates.active_form {
            task.active_form = Some(a);
        }
        if let Some(s) = updates.status {
            task.status = s;
        }
        if let Some(o) = updates.owner {
            task.owner = Some(o);
        }
        if let Some(blocks) = updates.add_blocks {
            for b in blocks {
                if !task.blocks.contains(&b) {
                    task.blocks.push(b);
                }
            }
        }
        if let Some(blocked_by) = updates.add_blocked_by {
            for b in blocked_by {
                if !task.blocked_by.contains(&b) {
                    task.blocked_by.push(b);
                }
            }
        }
        if let Some(meta) = updates.metadata {
            for (k, v) in meta {
                if v.is_null() {
                    task.metadata.remove(&k);
                } else {
                    task.metadata.insert(k, v);
                }
            }
        }

        // Atomic write
        let tmp_path = dir.join(format!("{}.json.tmp", task_id));
        let json = serde_json::to_string_pretty(&task)?;
        std::fs::write(&tmp_path, &json)?;
        std::fs::rename(&tmp_path, &task_path)?;

        Ok(task)
    }

    /// Delete a task.
    pub fn delete_task(&self, list_id: &str, task_id: &str) -> anyhow::Result<()> {
        let path = self.dir(list_id).join(format!("{}.json", task_id));
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(anyhow::anyhow!("Task {} not found", task_id))
            }
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> (TaskListStore, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        (TaskListStore::with_base(dir.path().to_path_buf()), dir)
    }

    #[test]
    fn test_create_and_get() {
        let (store, _dir) = test_store();
        let task = store
            .create_task("mylist", "Do thing", "A description", None, None)
            .unwrap();
        assert_eq!(task.id, "1");
        assert_eq!(task.subject, "Do thing");
        assert_eq!(task.status, TaskStatus::Pending);

        let fetched = store.get_task("mylist", "1").unwrap().unwrap();
        assert_eq!(fetched.id, "1");
        assert_eq!(fetched.subject, "Do thing");
        assert_eq!(fetched.description, "A description");
    }

    #[test]
    fn test_list_tasks() {
        let (store, _dir) = test_store();
        store
            .create_task("mylist", "First", "desc1", None, None)
            .unwrap();
        store
            .create_task("mylist", "Second", "desc2", None, None)
            .unwrap();

        let tasks = store.list_tasks("mylist").unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].subject, "First");
        assert_eq!(tasks[1].subject, "Second");
    }

    #[test]
    fn test_update_task() {
        let (store, _dir) = test_store();
        store
            .create_task("mylist", "Original", "desc", None, None)
            .unwrap();

        let mut meta = HashMap::new();
        meta.insert("key".to_string(), serde_json::json!("value"));

        let updated = store
            .update_task(
                "mylist",
                "1",
                TaskUpdate {
                    subject: Some("Updated".to_string()),
                    description: None,
                    active_form: Some("form1".to_string()),
                    status: Some(TaskStatus::InProgress),
                    owner: Some("alice".to_string()),
                    add_blocks: Some(vec!["2".to_string()]),
                    add_blocked_by: Some(vec!["3".to_string()]),
                    metadata: Some(meta),
                },
            )
            .unwrap();

        assert_eq!(updated.subject, "Updated");
        assert_eq!(updated.description, "desc");
        assert_eq!(updated.active_form.as_deref(), Some("form1"));
        assert_eq!(updated.status, TaskStatus::InProgress);
        assert_eq!(updated.owner.as_deref(), Some("alice"));
        assert_eq!(updated.blocks, vec!["2"]);
        assert_eq!(updated.blocked_by, vec!["3"]);
        assert_eq!(
            updated.metadata.get("key").unwrap(),
            &serde_json::json!("value")
        );
    }

    #[test]
    fn test_delete_task() {
        let (store, _dir) = test_store();
        store
            .create_task("mylist", "To delete", "desc", None, None)
            .unwrap();

        store.delete_task("mylist", "1").unwrap();
        assert!(store.get_task("mylist", "1").unwrap().is_none());

        // Deleting again should error
        assert!(store.delete_task("mylist", "1").is_err());
    }

    #[test]
    fn test_id_monotonic() {
        let (store, _dir) = test_store();
        let t1 = store.create_task("mylist", "A", "a", None, None).unwrap();
        let t2 = store.create_task("mylist", "B", "b", None, None).unwrap();
        let t3 = store.create_task("mylist", "C", "c", None, None).unwrap();

        assert_eq!(t1.id, "1");
        assert_eq!(t2.id, "2");
        assert_eq!(t3.id, "3");
    }

    #[test]
    fn test_list_empty() {
        let (store, _dir) = test_store();
        let tasks = store.list_tasks("nonexistent").unwrap();
        assert!(tasks.is_empty());
    }
}
