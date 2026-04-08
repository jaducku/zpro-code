# Auto-Update Feature Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement automatic version checking and safe binary updates using `self_update` crate, supporting both manual (`zpro-code update`) and automatic (on startup) update modes.

**Architecture:** UpdateManager struct handles all update logic (version check, installation, logging). Two execution paths use identical code: manual command triggers synchronous update, startup flow triggers async background update with exec() for process replacement.

**Tech Stack:** `self_update` crate (GitHub releases), `semver` for version comparison, `tokio` for async, `anyhow` for error handling.

---

## File Structure

```
src/
├── modules/
│   └── update_manager.rs       [NEW] UpdateManager struct, version check, install, logging
├── cli/
│   └── commands/
│       ├── mod.rs             [MODIFY] Export update command
│       └── update.rs           [NEW] `zpro-code update` subcommand
├── main.rs                      [MODIFY] Add auto-update on startup
└── config.rs                    [MODIFY] Add update log path helper

Cargo.toml                        [MODIFY] Add self_update + semver dependencies

tests/
└── update_manager_tests.rs      [NEW] UpdateManager unit tests
```

---

## Phase 1: Dependencies & Configuration

### Task 1: Add self_update and semver to Cargo.toml

**Files:**
- Modify: `Cargo.toml:23-68`

- [ ] **Step 1: Check current dependencies section**

```bash
grep -A 50 "^\[dependencies\]" Cargo.toml | head -20
```

- [ ] **Step 2: Add self_update and semver crates**

After existing dependencies, add:
```toml
self_update = "0.41"    # GitHub releases auto-detection + install
semver = "1"            # Version comparison
```

- [ ] **Step 3: Verify syntax**

```bash
cargo check
```

Expected: Should compile without errors.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: add self_update and semver dependencies"
```

---

## Phase 2: UpdateManager Module

### Task 2: Create update_manager.rs with UpdateManager struct

**Files:**
- Create: `src/modules/update_manager.rs`

- [ ] **Step 1: Create the file with struct definition**

```rust
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use chrono::Local;
use semver::Version;

#[derive(Clone)]
pub struct UpdateManager {
    repo_owner: &'static str,
    repo_name: &'static str,
    current_version: String,
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
        // Will implement in next task
        Ok(None)
    }

    /// Install update using self_update
    pub async fn install_update(&self) -> Result<()> {
        // Will implement in next task
        Ok(())
    }

    /// Log update status
    fn log_update(&self, status: &str, details: &str) -> Result<()> {
        // Will implement in next task
        Ok(())
    }
}
```

- [ ] **Step 2: Run cargo check**

```bash
cargo check --lib
```

Expected: Should compile.

- [ ] **Step 3: Commit**

```bash
git add src/modules/update_manager.rs
git commit -m "feat: create UpdateManager struct skeleton"
```

---

### Task 3: Implement version checking logic

**Files:**
- Modify: `src/modules/update_manager.rs:30-50`

- [ ] **Step 1: Add imports for GitHub API**

At top of file:
```rust
use reqwest::Client;
use serde_json::Value;
```

- [ ] **Step 2: Implement check_latest_version**

Replace the stub:
```rust
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
```

- [ ] **Step 3: Run cargo check**

```bash
cargo check --lib
```

Expected: Compiles.

- [ ] **Step 4: Commit**

```bash
git add src/modules/update_manager.rs
git commit -m "feat: implement version check from GitHub API"
```

---

### Task 4: Implement logging functionality

**Files:**
- Modify: `src/modules/update_manager.rs:55-75`

- [ ] **Step 1: Implement log_update method**

```rust
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
```

- [ ] **Step 2: Run cargo check**

```bash
cargo check --lib
```

- [ ] **Step 3: Commit**

```bash
git add src/modules/update_manager.rs
git commit -m "feat: implement update logging"
```

---

### Task 5: Implement install_update using self_update

**Files:**
- Modify: `src/modules/update_manager.rs:45-54`

- [ ] **Step 1: Add self_update import**

At top:
```rust
use self_update::backends::github;
```

- [ ] **Step 2: Implement install_update**

```rust
pub async fn install_update(&self) -> Result<()> {
    self.log_update("DOWNLOAD", &format!("Downloading v{} from releases", 
        self.check_latest_version().await?.unwrap_or_default()))?;

    // self_update handles:
    // - OS detection (macOS/Linux/Windows)
    // - Binary download
    // - Backup creation
    // - Atomic replacement
    // - Automatic rollback on failure
    let status = github::Update::configure()?
        .repo_owner(self.repo_owner)
        .repo_name(self.repo_name)
        .bin_name("zpro-code")
        .current_version(&self.current_version)
        .build()?
        .update()?;

    self.log_update("SUCCESS", &format!("Updated to {}", status.version()))?;
    Ok(())
}
```

- [ ] **Step 3: Run cargo check**

```bash
cargo check --lib
```

- [ ] **Step 4: Commit**

```bash
git add src/modules/update_manager.rs
git commit -m "feat: implement update installation with self_update"
```

---

## Phase 3: CLI Command

### Task 6: Create update subcommand

**Files:**
- Create: `src/cli/commands/update.rs`

- [ ] **Step 1: Create update.rs**

```rust
use clap::Subcommand;
use anyhow::Result;
use crate::modules::update_manager::UpdateManager;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum UpdateCommand {
    /// Check for and install updates
    #[command(about = "Check for and install latest version")]
    Latest,
}

pub async fn handle_update_command(update_log_path: PathBuf) -> Result<()> {
    let manager = UpdateManager::new(update_log_path);

    println!("✓ Checking for updates...");

    match manager.check_latest_version().await? {
        Some(latest_version) => {
            println!("✓ Downloading v{}...", latest_version);
            println!("✓ Installing...");
            manager.install_update().await?;
            println!("✓ Updated to v{}", latest_version);
        }
        None => {
            println!("✓ Already on latest version");
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Update cli/commands/mod.rs to export update**

Add to `src/cli/commands/mod.rs`:
```rust
pub mod update;
pub use update::handle_update_command;
```

- [ ] **Step 3: Run cargo check**

```bash
cargo check --lib
```

- [ ] **Step 4: Commit**

```bash
git add src/cli/commands/update.rs src/cli/commands/mod.rs
git commit -m "feat: add update subcommand"
```

---

## Phase 4: Main Integration

### Task 7: Add auto-update to main.rs startup

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Locate main() function and startup logic**

Find where TUI/app initialization happens (typically in async main).

- [ ] **Step 2: Add auto-update check before app start**

After CLI parsing, before app initialization:

```rust
// Import at top
use crate::modules::update_manager::UpdateManager;

// In main app startup flow (after CLI parsing):
let config_path = /* existing config path logic */;
let log_path = config_path.join("updates.log");

let manager = UpdateManager::new(log_path);

// Non-blocking version check with timeout
match tokio::time::timeout(
    std::time::Duration::from_secs(2),
    manager.check_latest_version()
).await {
    Ok(Ok(Some(latest))) => {
        // New version available - spawn background update
        println!("↻ Updating to v{}...", latest);
        
        let manager_update = UpdateManager::new(log_path.clone());
        tokio::spawn(async move {
            if let Err(e) = manager_update.install_update().await {
                eprintln!("Update failed: {}", e);
            }
        });
        
        // Brief wait for update to start, then continue
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    _ => {
        // Timeout, check failed, or no update - just continue
    }
}

// Continue with normal TUI startup
run_tui().await?;
```

- [ ] **Step 3: Run cargo check**

```bash
cargo check
```

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: add auto-update check on startup"
```

---

## Phase 5: Testing

### Task 8: Write unit tests for UpdateManager

**Files:**
- Create: `tests/update_manager_tests.rs`

- [ ] **Step 1: Create test file**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manager() -> (UpdateManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("updates.log");
        let manager = UpdateManager::new(log_path);
        (manager, temp_dir)
    }

    #[test]
    fn test_update_manager_initialization() {
        let (manager, _temp) = create_test_manager();
        assert_eq!(manager.repo_owner, "jaducku");
        assert_eq!(manager.repo_name, "zpro-code-releases");
    }

    #[test]
    fn test_logging_creates_file() {
        let (manager, _temp) = create_test_manager();
        manager.log_update("TEST", "test message").unwrap();
        
        assert!(manager.log_path.exists());
        let content = std::fs::read_to_string(&manager.log_path).unwrap();
        assert!(content.contains("TEST"));
        assert!(content.contains("test message"));
    }

    #[test]
    fn test_logging_appends_multiple_entries() {
        let (manager, _temp) = create_test_manager();
        manager.log_update("ENTRY1", "first").unwrap();
        manager.log_update("ENTRY2", "second").unwrap();
        
        let content = std::fs::read_to_string(&manager.log_path).unwrap();
        assert!(content.contains("ENTRY1"));
        assert!(content.contains("ENTRY2"));
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --lib update_manager
```

Expected: Tests pass.

- [ ] **Step 3: Commit**

```bash
git add tests/update_manager_tests.rs
git commit -m "test: add UpdateManager unit tests"
```

---

## Phase 6: Integration & Verification

### Task 9: Verify compilation and basic functionality

**Files:**
- (No new files, verification only)

- [ ] **Step 1: Clean build**

```bash
cargo clean && cargo build --release
```

Expected: Compiles successfully.

- [ ] **Step 2: Test update command syntax**

```bash
./target/release/zpro-code update --help
```

Expected: Shows help for update command.

- [ ] **Step 3: Manual test (if safe)**

```bash
# Test version check without installing
# Would need additional test mode or dry-run flag
```

- [ ] **Step 4: Verify logs**

```bash
cat ~/.zpro-code/updates.log
```

Expected: Should show update attempts.

- [ ] **Step 5: Final commit**

```bash
git add .
git commit -m "chore: verify auto-update feature builds and runs"
```

---

## Success Criteria

- [x] `self_update` and `semver` added to Cargo.toml
- [x] UpdateManager struct with version check, install, logging
- [x] GitHub API version detection works
- [x] self_update installs binary safely
- [x] Logging to ~/.zpro-code/updates.log
- [x] `zpro-code update` command works
- [x] Auto-update on startup logic integrated
- [x] All tests pass
- [x] Compiles without errors
- [x] Ready for manual testing across platforms

---

## Notes for Implementation

1. **Error Handling:** Always log errors, never crash. Network failures should not block app startup.
2. **Permissions:** self_update handles file permissions automatically.
3. **Windows Compatibility:** self_update handles .exe naming automatically.
4. **Rollback:** self_update automatically creates backup and restores on failure.
5. **Testing:** Can't test actual updates in unit tests, but logging and version comparison can be tested.
6. **Performance:** Startup version check should timeout after 5 seconds to not block TUI.

---
