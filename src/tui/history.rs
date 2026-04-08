use std::path::PathBuf;

pub struct InputHistory {
    pub entries: Vec<String>,
    pub cursor: Option<usize>,
    pub stashed: Option<String>,
    max_entries: usize,
    file_path: Option<PathBuf>,
}

impl InputHistory {
    pub fn new(file_path: PathBuf, max_entries: usize) -> Self {
        let entries = std::fs::read_to_string(&file_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default();
        Self {
            entries,
            cursor: None,
            stashed: None,
            max_entries,
            file_path: Some(file_path),
        }
    }

    pub fn new_in_memory(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: None,
            stashed: None,
            max_entries,
            file_path: None,
        }
    }

    pub fn push(&mut self, entry: String) {
        if entry.is_empty() {
            return;
        }
        if self.entries.last() == Some(&entry) {
            return;
        }
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.save();
    }

    pub fn up(&mut self, current_input: &str) -> Option<&str> {
        match self.cursor {
            None => {
                if self.entries.is_empty() {
                    return None;
                }
                self.stashed = Some(current_input.to_string());
                let idx = self.entries.len() - 1;
                self.cursor = Some(idx);
                Some(&self.entries[idx])
            }
            Some(idx) => {
                if idx == 0 {
                    return None;
                }
                let new_idx = idx - 1;
                self.cursor = Some(new_idx);
                Some(&self.entries[new_idx])
            }
        }
    }

    pub fn down(&mut self) -> Option<&str> {
        let idx = self.cursor?;
        if idx + 1 < self.entries.len() {
            let new_idx = idx + 1;
            self.cursor = Some(new_idx);
            Some(&self.entries[new_idx])
        } else {
            self.cursor = None;
            // Return stashed input
            self.stashed.as_deref()
        }
    }

    pub fn reset(&mut self) {
        self.cursor = None;
        self.stashed = None;
    }

    fn save(&self) {
        if let Some(ref path) = self.file_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(
                path,
                serde_json::to_string(&self.entries).unwrap_or_default(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_up() {
        let mut h = InputHistory::new_in_memory(500);
        h.push("first".into());
        h.push("second".into());
        assert_eq!(h.up("current"), Some("second"));
        assert_eq!(h.up("current"), Some("first"));
        assert_eq!(h.up("current"), None);
    }

    #[test]
    fn test_down_restores_stashed() {
        let mut h = InputHistory::new_in_memory(500);
        h.push("old".into());
        let _ = h.up("typing");
        assert_eq!(h.down(), Some("typing"));
        assert_eq!(h.down(), None);
    }

    #[test]
    fn test_reset() {
        let mut h = InputHistory::new_in_memory(500);
        h.push("entry".into());
        let _ = h.up("x");
        h.reset();
        assert_eq!(h.cursor, None);
        assert_eq!(h.stashed, None);
    }

    #[test]
    fn test_max_entries() {
        let mut h = InputHistory::new_in_memory(3);
        h.push("a".into());
        h.push("b".into());
        h.push("c".into());
        h.push("d".into());
        assert_eq!(h.entries.len(), 3);
        assert_eq!(h.entries[0], "b");
    }

    #[test]
    fn test_no_duplicate_consecutive() {
        let mut h = InputHistory::new_in_memory(500);
        h.push("same".into());
        h.push("same".into());
        assert_eq!(h.entries.len(), 1);
    }

    #[test]
    fn test_file_persistence() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let mut h = InputHistory::new(path.clone(), 500);
        h.push("hello".into());
        h.push("world".into());
        let h2 = InputHistory::new(path, 500);
        assert_eq!(h2.entries, vec!["hello", "world"]);
    }
}
