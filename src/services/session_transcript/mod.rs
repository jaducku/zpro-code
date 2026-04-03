use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::types::message::ApiMessage;

#[derive(Serialize, Deserialize)]
pub struct SessionTranscript {
    pub session_id: String,
    pub messages: Vec<ApiMessage>,
    pub model: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct SessionSummary {
    pub session_id: String,
    pub model: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
}

pub struct SessionStore {
    base_dir: PathBuf,
}

impl SessionStore {
    pub fn new() -> anyhow::Result<Self> {
        let home = std::env::var("HOME")
            .map_err(|_| anyhow::anyhow!("HOME environment variable not set"))?;
        let base_dir = PathBuf::from(home).join(".claude").join("sessions");
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn save(&self, transcript: &SessionTranscript) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.base_dir)?;
        let path = self.base_dir.join(format!("{}.json", transcript.session_id));
        let json = serde_json::to_string_pretty(transcript)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn load(&self, session_id: &str) -> anyhow::Result<SessionTranscript> {
        let path = self.base_dir.join(format!("{}.json", session_id));
        let json = std::fs::read_to_string(&path)
            .map_err(|_| anyhow::anyhow!("Session not found: {}", session_id))?;
        let transcript: SessionTranscript = serde_json::from_str(&json)?;
        Ok(transcript)
    }

    pub fn list(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let mut summaries = Vec::new();
        if !self.base_dir.exists() {
            return Ok(summaries);
        }
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let json = std::fs::read_to_string(&path)?;
                if let Ok(transcript) = serde_json::from_str::<SessionTranscript>(&json) {
                    summaries.push(SessionSummary {
                        session_id: transcript.session_id,
                        model: transcript.model,
                        created_at: transcript.created_at,
                        message_count: transcript.messages.len(),
                    });
                }
            }
        }
        Ok(summaries)
    }

    pub fn delete(&self, session_id: &str) -> anyhow::Result<()> {
        let path = self.base_dir.join(format!("{}.json", session_id));
        std::fs::remove_file(&path)
            .map_err(|_| anyhow::anyhow!("Session not found: {}", session_id))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::content::ContentBlock;
    use crate::types::message::Role;
    use tempfile::TempDir;

    fn make_transcript(session_id: &str) -> SessionTranscript {
        let now = chrono::Utc::now();
        SessionTranscript {
            session_id: session_id.to_string(),
            messages: vec![ApiMessage {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "hello".into(),
                }],
            }],
            model: "claude-sonnet".to_string(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn test_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let store = SessionStore::with_base_dir(tmp.path().to_path_buf());
        let transcript = make_transcript("sess-1");
        store.save(&transcript).unwrap();
        let loaded = store.load("sess-1").unwrap();
        assert_eq!(loaded.session_id, "sess-1");
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.model, "claude-sonnet");
    }

    #[test]
    fn test_list() {
        let tmp = TempDir::new().unwrap();
        let store = SessionStore::with_base_dir(tmp.path().to_path_buf());
        store.save(&make_transcript("sess-1")).unwrap();
        store.save(&make_transcript("sess-2")).unwrap();
        store.save(&make_transcript("sess-3")).unwrap();
        let list = store.list().unwrap();
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn test_load_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let store = SessionStore::with_base_dir(tmp.path().to_path_buf());
        let result = store.load("nonexistent");
        assert!(result.is_err());
    }
}
