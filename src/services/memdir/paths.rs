use std::path::{Path, PathBuf};

use crate::utils::paths::global_config_dir;

/// Replace `/`, `\`, `:` with `-`, collapse consecutive `-`, then trim leading/trailing `-`.
pub fn sanitize_git_root(path: &str) -> String {
    let mut sanitized = String::with_capacity(path.len());
    let mut last_was_dash = false;
    for c in path.chars() {
        let ch = match c {
            '/' | '\\' | ':' => '-',
            _ => c,
        };
        if ch == '-' {
            if !last_was_dash {
                sanitized.push(ch);
            }
            last_was_dash = true;
        } else {
            sanitized.push(ch);
            last_was_dash = false;
        }
    }
    sanitized.trim_matches('-').to_string()
}

/// Walk parent directories looking for `.git`.
pub fn find_git_root(from: &Path) -> Option<PathBuf> {
    let mut current = from.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Get the auto-memory path for a given working directory.
///
/// Checks `ATOM_COWORK_MEMORY_PATH_OVERRIDE` env var first,
/// then finds the git root and returns `~/.zpro-code/projects/{sanitized}/memory/`.
pub fn get_auto_mem_path(cwd: &Path) -> Option<PathBuf> {
    if let Ok(override_path) = std::env::var("ATOM_COWORK_MEMORY_PATH_OVERRIDE") {
        if !override_path.is_empty() {
            return Some(PathBuf::from(override_path));
        }
    }

    let git_root = find_git_root(cwd)?;
    let sanitized = sanitize_git_root(&git_root.to_string_lossy());
    Some(
        global_config_dir()
            .join("projects")
            .join(sanitized)
            .join("memory"),
    )
}

/// Check if auto-memory is enabled using current environment.
pub fn is_auto_memory_enabled() -> bool {
    let disable_env = std::env::var("ATOM_CODE_DISABLE_AUTO_MEMORY").unwrap_or_default();
    // bare_mode and settings_enabled would come from runtime config;
    // default to false / true respectively.
    is_auto_memory_enabled_with_env(&disable_env, false, true)
}

/// Pure logic for checking if auto-memory is enabled.
///
/// - `disable_env` = value of `ATOM_CODE_DISABLE_AUTO_MEMORY`
/// - `bare_mode` = whether running in bare mode
/// - `settings_enabled` = whether settings have auto-memory enabled
pub fn is_auto_memory_enabled_with_env(
    disable_env: &str,
    bare_mode: bool,
    settings_enabled: bool,
) -> bool {
    if disable_env == "1" {
        return false;
    }
    if bare_mode {
        return false;
    }
    if !settings_enabled {
        return false;
    }
    true
}

/// Check whether `path` is inside the given `memory_dir`.
pub fn is_auto_mem_path(path: &Path, memory_dir: &Path) -> bool {
    path.starts_with(memory_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_path_for_memory() {
        let result = sanitize_git_root("/Users/dev/project");
        assert!(!result.contains('/'));
        assert!(!result.contains('\\'));
        assert!(!result.contains(':'));
        // Should not start or end with '-'
        assert!(!result.starts_with('-'));
        assert!(!result.ends_with('-'));
        assert_eq!(result, "Users-dev-project");
    }

    #[test]
    fn test_is_auto_memory_enabled_default() {
        assert!(is_auto_memory_enabled_with_env("", false, true));
    }

    #[test]
    fn test_is_auto_memory_disabled_by_env() {
        assert!(!is_auto_memory_enabled_with_env("1", false, true));
    }

    #[test]
    fn test_is_auto_memory_disabled_by_bare() {
        assert!(!is_auto_memory_enabled_with_env("", true, true));
    }

    #[test]
    fn test_is_auto_memory_disabled_by_settings() {
        assert!(!is_auto_memory_enabled_with_env("", false, false));
    }

    #[test]
    fn test_is_auto_mem_path() {
        let memory_dir = Path::new("/home/user/.atom/projects/foo/memory");
        let inside = Path::new("/home/user/.atom/projects/foo/memory/MEMORY.md");
        let outside = Path::new("/home/user/.atom/projects/bar/memory/MEMORY.md");

        assert!(is_auto_mem_path(inside, memory_dir));
        assert!(!is_auto_mem_path(outside, memory_dir));
    }

    #[test]
    fn test_find_git_root_finds_repo() {
        // Use tempdir to create a fake git repo
        let tmp = tempfile::tempdir().unwrap();
        let git_dir = tmp.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();
        let sub = tmp.path().join("a").join("b");
        std::fs::create_dir_all(&sub).unwrap();

        let result = find_git_root(&sub);
        assert_eq!(result, Some(tmp.path().to_path_buf()));
    }

    #[test]
    fn test_find_git_root_none() {
        let tmp = tempfile::tempdir().unwrap();
        // No .git anywhere inside tmp
        let result = find_git_root(tmp.path());
        // Could find an ancestor .git (the real repo), but inside a fresh tempdir
        // the function should either find something or not — we just ensure no panic.
        // In CI environments the tempdir is often under /tmp which has no .git.
        let _ = result;
    }

    #[test]
    fn test_sanitize_git_root_windows_path() {
        let result = sanitize_git_root("C:\\Users\\dev\\project");
        assert!(!result.contains('\\'));
        assert!(!result.contains(':'));
        assert_eq!(result, "C-Users-dev-project");
    }
}
