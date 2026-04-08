use anyhow::{anyhow, Result};
use chrono::Local;
use reqwest::Client;
use semver::Version;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone)]
pub struct UpdateManager {
    repo_owner: &'static str,
    repo_name: &'static str,
    current_version: String,
    #[allow(dead_code)]
    log_path: PathBuf,
}

impl UpdateManager {
    pub fn new(log_path: PathBuf) -> Self {
        Self {
            repo_owner: "jaducku",
            repo_name: "zpro-code-releases",
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            log_path,
        }
    }

    /// Check for latest version on GitHub
    pub async fn check_latest_version(&self) -> Result<Option<String>> {
        let client = Client::new();
        let url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.repo_owner, self.repo_name
        );

        // Add User-Agent header (GitHub API requires it)
        let response = client
            .get(&url)
            .header("User-Agent", "zpro-code-updater")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await?;

        if !response.status().is_success() {
            self.log_update("CHECK_FAILED", &format!("GitHub API error: {}", response.status()))?;
            return Ok(None);
        }

        let data: Value = response.json().await?;
        let tag_name = data
            .get("tag_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No tag_name in response"))?
            .trim_start_matches('v');

        // Check if newer version exists
        let current = Version::parse(&self.current_version)?;
        let latest = Version::parse(tag_name)?;

        if latest > current {
            self.log_update("CHECK", &format!("Current {}, Latest: {}", current, latest))?;
            Ok(Some(tag_name.to_string()))
        } else {
            Ok(None)
        }
    }

    /// Install update using self_update
    pub async fn install_update(&self) -> Result<()> {
        // Will implement in next task
        Ok(())
    }

    /// Log update status
    fn log_update(&self, status: &str, details: &str) -> Result<()> {
        use std::fs::OpenOptions;
        use std::io::Write;

        // Create log directory if it doesn't exist
        if let Some(parent) = self.log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
        let log_line = format!("[{}] {}: {}\n", timestamp, status, details);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        file.write_all(log_line.as_bytes())?;
        Ok(())
    }
}
