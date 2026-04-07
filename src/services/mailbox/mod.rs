pub mod types;

pub use types::{MailboxMessage, StructuredMessage};

use std::path::PathBuf;

pub struct Mailbox {
    base_dir: PathBuf,
}

impl Mailbox {
    pub fn new(team_name: &str) -> Self {
        let base = crate::utils::paths::global_config_dir()
            .join("teams")
            .join(team_name)
            .join("inboxes");
        Self { base_dir: base }
    }

    fn inbox_path(&self, agent_name: &str) -> PathBuf {
        self.base_dir.join(format!("{}.json", agent_name))
    }

    /// Write a message to the recipient's inbox.
    /// Uses fs2 exclusive file locking for concurrency safety.
    pub fn write(&self, recipient: &str, msg: MailboxMessage) -> anyhow::Result<()> {
        use fs2::FileExt;

        std::fs::create_dir_all(&self.base_dir)?;

        let path = self.inbox_path(recipient);
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        file.lock_exclusive()?;

        // Read existing messages
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let mut messages: Vec<MailboxMessage> = if content.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&content)?
        };

        messages.push(msg);

        // Write back
        let json = serde_json::to_string_pretty(&messages)?;
        std::fs::write(&path, json)?;

        file.unlock()?;
        Ok(())
    }

    /// Read all messages from an agent's inbox.
    /// Returns an empty vec if the inbox file doesn't exist.
    pub fn read(&self, agent_name: &str) -> anyhow::Result<Vec<MailboxMessage>> {
        let path = self.inbox_path(agent_name);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    Ok(Vec::new())
                } else {
                    Ok(serde_json::from_str(&content)?)
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Clear an agent's inbox by removing the file.
    pub fn clear(&self, agent_name: &str) -> anyhow::Result<()> {
        let path = self.inbox_path(agent_name);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    fn test_mailbox(dir: &TempDir) -> Mailbox {
        Mailbox {
            base_dir: dir.path().to_path_buf(),
        }
    }

    #[test]
    fn test_write_and_read() {
        let dir = TempDir::new().unwrap();
        let mailbox = test_mailbox(&dir);

        let msg = MailboxMessage {
            from: "alice".to_string(),
            text: "Hello Bob".to_string(),
            summary: None,
            timestamp: Utc::now(),
        };

        mailbox.write("bob", msg.clone()).unwrap();

        let messages = mailbox.read("bob").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].from, "alice");
        assert_eq!(messages[0].text, "Hello Bob");
    }

    #[test]
    fn test_read_empty() {
        let dir = TempDir::new().unwrap();
        let mailbox = test_mailbox(&dir);

        let messages = mailbox.read("nonexistent").unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn test_write_multiple_and_clear() {
        let dir = TempDir::new().unwrap();
        let mailbox = test_mailbox(&dir);

        for i in 0..3 {
            let msg = MailboxMessage {
                from: format!("agent_{}", i),
                text: format!("Message {}", i),
                summary: None,
                timestamp: Utc::now(),
            };
            mailbox.write("bob", msg).unwrap();
        }

        let messages = mailbox.read("bob").unwrap();
        assert_eq!(messages.len(), 3);

        mailbox.clear("bob").unwrap();

        let messages = mailbox.read("bob").unwrap();
        assert!(messages.is_empty());
    }
}
