use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeSession {
    pub original_cwd: PathBuf,
    pub worktree_path: PathBuf,
    pub worktree_name: String,
    pub worktree_branch: Option<String>,
    pub original_branch: Option<String>,
    pub original_head_commit: Option<String>,
    pub session_id: String,
}

pub struct RemoveResult {
    pub discarded_files: u32,
    pub discarded_commits: u32,
}

pub struct WorktreeService;

impl WorktreeService {
    /// Validate a worktree slug: [a-zA-Z0-9._-], max 64 chars,
    /// no `..`, no leading `-` or `.`, no `/`.
    pub fn validate_slug(slug: &str) -> Result<(), String> {
        if slug.is_empty() {
            return Err("slug must not be empty".into());
        }
        if slug.len() > 64 {
            return Err("slug must be at most 64 characters".into());
        }
        if slug.starts_with('-') || slug.starts_with('.') {
            return Err("slug must not start with '-' or '.'".into());
        }
        if slug.contains("..") {
            return Err("slug must not contain '..'".into());
        }
        if slug.contains('/') {
            return Err("slug must not contain '/'".into());
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
        {
            return Err("slug must only contain [a-zA-Z0-9._-]".into());
        }
        Ok(())
    }

    /// Find the git root for the given working directory.
    pub async fn find_git_root(cwd: &Path) -> Result<PathBuf, String> {
        let output = tokio::process::Command::new("git")
            .args(["-C", &cwd.to_string_lossy(), "rev-parse", "--show-toplevel"])
            .output()
            .await
            .map_err(|e| format!("failed to run git: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("not a git repository: {stderr}"));
        }

        let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(root))
    }

    /// Create a new worktree. Returns the WorktreeSession on success.
    pub async fn create(
        cwd: &Path,
        slug: &str,
        session_id: &str,
    ) -> Result<WorktreeSession, String> {
        Self::validate_slug(slug)?;

        let git_root = Self::find_git_root(cwd).await?;

        // Get current branch
        let branch_output = tokio::process::Command::new("git")
            .args(["-C", &git_root.to_string_lossy(), "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .await
            .map_err(|e| format!("failed to get current branch: {e}"))?;
        let original_branch = if branch_output.status.success() {
            Some(String::from_utf8_lossy(&branch_output.stdout).trim().to_string())
        } else {
            None
        };

        // Get HEAD commit
        let head_output = tokio::process::Command::new("git")
            .args(["-C", &git_root.to_string_lossy(), "rev-parse", "HEAD"])
            .output()
            .await
            .map_err(|e| format!("failed to get HEAD: {e}"))?;
        let original_head_commit = if head_output.status.success() {
            Some(String::from_utf8_lossy(&head_output.stdout).trim().to_string())
        } else {
            None
        };

        let worktree_branch = format!("worktree-{slug}");
        let worktree_path = git_root
            .parent()
            .unwrap_or(&git_root)
            .join(".claude")
            .join("worktrees")
            .join(slug);

        let output = tokio::process::Command::new("git")
            .args([
                "-C",
                &git_root.to_string_lossy(),
                "worktree",
                "add",
                "-B",
                &worktree_branch,
                &worktree_path.to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| format!("failed to create worktree: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git worktree add failed: {stderr}"));
        }

        Ok(WorktreeSession {
            original_cwd: cwd.to_path_buf(),
            worktree_path,
            worktree_name: slug.to_string(),
            worktree_branch: Some(worktree_branch),
            original_branch,
            original_head_commit,
            session_id: session_id.to_string(),
        })
    }

    /// Count uncommitted files in a worktree path.
    pub async fn count_uncommitted(path: &Path) -> Result<u32, String> {
        let output = tokio::process::Command::new("git")
            .args(["-C", &path.to_string_lossy(), "status", "--porcelain"])
            .output()
            .await
            .map_err(|e| format!("failed to run git status: {e}"))?;

        if !output.status.success() {
            return Ok(0);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let count = stdout.lines().filter(|l| !l.is_empty()).count() as u32;
        Ok(count)
    }

    /// Count new commits since a given head commit.
    pub async fn count_new_commits(path: &Path, head: &str) -> Result<u32, String> {
        let output = tokio::process::Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "rev-list",
                &format!("{head}..HEAD"),
                "--count",
            ])
            .output()
            .await
            .map_err(|e| format!("failed to count commits: {e}"))?;

        if !output.status.success() {
            return Ok(0);
        }

        let count_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        count_str.parse::<u32>().map_err(|e| format!("failed to parse commit count: {e}"))
    }

    /// Remove a worktree and its branch. Returns stats about discarded work.
    pub async fn remove(session: &WorktreeSession) -> Result<RemoveResult, String> {
        let git_root = Self::find_git_root(&session.original_cwd).await?;

        // Count uncommitted files
        let discarded_files = Self::count_uncommitted(&session.worktree_path)
            .await
            .unwrap_or(0);

        // Count new commits
        let discarded_commits = if let Some(ref head) = session.original_head_commit {
            Self::count_new_commits(&session.worktree_path, head)
                .await
                .unwrap_or(0)
        } else {
            0
        };

        // Remove worktree
        let output = tokio::process::Command::new("git")
            .args([
                "-C",
                &git_root.to_string_lossy(),
                "worktree",
                "remove",
                "--force",
                &session.worktree_path.to_string_lossy(),
            ])
            .output()
            .await
            .map_err(|e| format!("failed to remove worktree: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git worktree remove failed: {stderr}"));
        }

        // Delete branch
        if let Some(ref branch) = session.worktree_branch {
            let _ = tokio::process::Command::new("git")
                .args(["-C", &git_root.to_string_lossy(), "branch", "-D", branch])
                .output()
                .await;
        }

        Ok(RemoveResult {
            discarded_files,
            discarded_commits,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_slug_valid() {
        assert!(WorktreeService::validate_slug("my-feature").is_ok());
        assert!(WorktreeService::validate_slug("test_123").is_ok());
        assert!(WorktreeService::validate_slug("v1.0.0").is_ok());
    }

    #[test]
    fn test_validate_slug_invalid() {
        // Empty
        assert!(WorktreeService::validate_slug("").is_err());
        // Leading dash
        assert!(WorktreeService::validate_slug("-starts-dash").is_err());
        // Leading dot
        assert!(WorktreeService::validate_slug(".hidden").is_err());
        // Contains slash
        assert!(WorktreeService::validate_slug("has/slash").is_err());
        // Path traversal
        assert!(WorktreeService::validate_slug("path..traversal").is_err());
        // Contains space
        assert!(WorktreeService::validate_slug("has space").is_err());
        // Too long (65 chars)
        let long = "a".repeat(65);
        assert!(WorktreeService::validate_slug(&long).is_err());
    }
}
