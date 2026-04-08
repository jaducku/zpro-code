---
title: Auto-Update Feature Design
date: 2026-04-08
status: draft
---

# Auto-Update Feature Design

## Overview

Implement automatic update checking and installation for `zpro-code` CLI using the `self_update` crate, with two execution modes:
1. **Manual**: `zpro-code update` command
2. **Automatic**: Check and update on startup

## Architecture

### Core Components

```rust
// src/modules/update_manager.rs
pub struct UpdateManager {
    repo_owner: &'static str,      // "jaducku"
    repo_name: &'static str,       // "zpro-code-releases"  
    current_version: String,
    log_path: PathBuf,             // ~/.zpro-code/updates.log
}

impl UpdateManager {
    // Check latest version from GitHub Releases
    pub async fn check_latest_version() -> Result<String>
    
    // Install update using self_update crate
    pub async fn install_update() -> Result<()>
    
    // Log update status
    fn log_update(status: &str, details: &str) -> Result<()>
}
```

### File Structure

```
src/
├── modules/
│   └── update_manager.rs        # UpdateManager implementation
├── cli/
│   └── commands/
│       └── update.rs            # `zpro-code update` command
└── main.rs                       # Auto-update on startup
```

## Execution Flows

### Mode 1: Manual Update (`zpro-code update`)

```
User runs: zpro-code update
    ↓
Check latest version (GitHub Releases)
    ↓
New version available?
    ├─ NO: Show "Already on latest version"
    └─ YES:
        ↓
        Spawn separate process for update
        ↓
        self_update crate handles:
        - OS detection (macOS/Linux/Windows)
        - Download binary
        - Backup original
        - Atomic replacement
        - Rollback on failure
        ↓
        Log: [timestamp] UPDATE_START/PROGRESS/SUCCESS
        ↓
        Terminate
```

### Mode 2: Auto Update on Startup (`zpro-code`)

```
User runs: zpro-code
    ↓
Check latest version (GitHub Releases)
    ↓
New version available?
    ├─ NO:
    │   └─ Run TUI normally
    └─ YES:
        ↓
        Show spinner "Updating..."
        ↓
        Spawn separate process for update
        ↓
        self_update crate (same as Mode 1)
        ↓
        Log update status
        ↓
        Current process terminates
        ↓
        Exec new binary with original args
        ↓
        New process starts TUI
```

## Implementation Details

### Key Principle

**Running process never updates itself.**

- Separate process handles all update operations
- Current process terminates cleanly
- `exec()` replaces current process with new binary (no file locking issues)
- Works reliably on Windows/macOS/Linux

### Dependencies

```toml
[dependencies]
self_update = "0.41"    # GitHub auto-detection + install
semver = "1"            # Version comparison
```

### Update Log Format

**Location**: `~/.zpro-code/updates.log`

```
[2026-04-08 10:30:45] CHECK: Current 0.1.9, Latest: 0.2.0
[2026-04-08 10:30:46] DOWNLOAD: zpro-code v0.2.0 (macOS-arm64)
[2026-04-08 10:30:48] BACKUP: Created backup at /usr/local/bin/zpro-code.bak
[2026-04-08 10:30:49] INSTALL: Replaced binary
[2026-04-08 10:30:50] SUCCESS: Update complete, restarting...
[2026-04-08 10:30:51] RESTART: Launching new binary
```

### User Feedback

**Mode 1 (Manual)**
```
$ zpro-code update
✓ Checking for updates...
✓ Downloading v0.2.0
✓ Installing...
✓ Updated to v0.2.0
```

**Mode 2 (Auto)**
```
[TUI starts]
[Spinner shows briefly: "Checking for updates..."]
[If update: "Updating..." → silence → TUI restarts with new version]
[If no update: TUI starts immediately]
```

## Safety & Stability

### Why This Approach is Safe

1. **No Runtime Binary Modification**
   - Separate process handles update
   - Running process never touches itself
   - Avoids file locking on Windows

2. **Atomic Replacement**
   - `self_update` creates backup before replacing
   - Automatic rollback on failure
   - OS-specific edge cases handled

3. **Clean Process Termination**
   - Current process logs and exits gracefully
   - `exec()` replaces current process memory
   - No zombie processes or lingering state

4. **Consistent Logic**
   - Manual and auto modes use identical code
   - Single code path reduces bugs
   - Easy to test and maintain

## Testing Strategy

1. **Unit Tests**
   - Version comparison logic
   - Logging functions
   - Configuration loading

2. **Integration Tests**
   - Mock GitHub API responses
   - Verify update detection
   - Test with dummy binaries

3. **Manual Testing**
   - Test on macOS, Linux, Windows (if available)
   - Verify rollback on simulated failure
   - Check log output format

## Error Handling

**Update Check Fails**
- Log error, continue normally (non-blocking)
- Don't bother user with network errors

**Update Installation Fails**
- Log error details
- Rollback to previous binary
- User can retry with `zpro-code update`

**Network Issues**
- Timeout after 5 seconds
- Fallback to cached version info
- Log attempt

## Future Considerations

- Scheduled background updates (24-hour check interval)
- Update notifications in TUI
- Beta/nightly channel support
- Custom binary download sources

## Success Criteria

- [x] Version check from GitHub Releases works
- [x] `zpro-code update` installs new version
- [x] Startup auto-update triggers correctly
- [x] Binary replaced atomically (no corruption)
- [x] Rollback works on failure
- [x] All updates logged to ~/.zpro-code/updates.log
- [x] Works on macOS, Linux, Windows
- [x] No UX disruption on failed updates
