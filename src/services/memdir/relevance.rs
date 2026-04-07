use std::collections::HashSet;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use super::scan::{format_memory_manifest, scan_memory_files};
use super::types::RelevantMemory;

const MAX_SELECTED_MEMORIES: usize = 5;

/// Find relevant memory files for a given query.
///
/// Uses an optional LLM selector callback to pick the most relevant files.
/// Falls back to most-recent files when no selector is provided or on error.
pub async fn find_relevant_memories(
    query: &str,
    memory_dir: &Path,
    _recent_tools: &[String],
    already_surfaced: &HashSet<String>,
    llm_selector: Option<
        &(dyn for<'a> Fn(
            &'a str,
            &'a str,
        )
            -> Pin<Box<dyn Future<Output = Result<Vec<String>, String>> + Send + 'a>>
              + Send
              + Sync),
    >,
) -> Vec<RelevantMemory> {
    let headers = scan_memory_files(memory_dir);
    if headers.is_empty() {
        return Vec::new();
    }

    // Filter out already-surfaced files
    let candidates: Vec<_> = headers
        .into_iter()
        .filter(|h| !already_surfaced.contains(&h.filename))
        .collect();

    if candidates.is_empty() {
        return Vec::new();
    }

    let selected_filenames: Option<Vec<String>> = if let Some(selector) = llm_selector {
        let manifest = format_memory_manifest(&candidates);
        match selector(query, &manifest).await {
            Ok(filenames) => Some(filenames),
            Err(_) => None, // graceful degradation → fall back to recency
        }
    } else {
        None
    };

    let results: Vec<RelevantMemory> = match selected_filenames {
        Some(filenames) => {
            // Map selected filenames back to RelevantMemory, preserving LLM order
            filenames
                .iter()
                .filter_map(|name| {
                    candidates
                        .iter()
                        .find(|c| &c.filename == name)
                        .map(|c| RelevantMemory {
                            path: c.file_path.clone(),
                            mtime_ms: c.mtime_ms,
                        })
                })
                .collect()
        }
        None => {
            // Candidates are already sorted by mtime descending from scan_memory_files
            candidates
                .iter()
                .map(|c| RelevantMemory {
                    path: c.file_path.clone(),
                    mtime_ms: c.mtime_ms,
                })
                .collect()
        }
    };

    results.into_iter().take(MAX_SELECTED_MEMORIES).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_md(dir: &Path, name: &str, frontmatter: &str) {
        let content = format!("---\n{frontmatter}\n---\n# Content");
        std::fs::write(dir.join(name), content).unwrap();
    }

    #[tokio::test]
    async fn test_find_relevant_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let result =
            find_relevant_memories("test query", tmp.path(), &[], &HashSet::new(), None).await;
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_find_relevant_excludes_surfaced() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        write_md(dir, "alpha.md", "description: Alpha\ntype: project");
        write_md(dir, "beta.md", "description: Beta\ntype: project");

        let mut surfaced = HashSet::new();
        surfaced.insert("alpha.md".to_string());

        let result = find_relevant_memories("test query", dir, &[], &surfaced, None).await;

        assert_eq!(result.len(), 1);
        assert!(result[0].path.ends_with("beta.md"));
    }

    #[tokio::test]
    async fn test_find_relevant_no_llm_returns_recent() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        write_md(dir, "a.md", "description: A\ntype: project");
        write_md(dir, "b.md", "description: B\ntype: feedback");
        write_md(dir, "c.md", "description: C\ntype: user");

        let result = find_relevant_memories("test query", dir, &[], &HashSet::new(), None).await;

        // All 3 returned (< MAX_SELECTED_MEMORIES)
        assert_eq!(result.len(), 3);
    }

    #[tokio::test]
    async fn test_find_relevant_respects_max() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        for i in 0..10 {
            write_md(
                dir,
                &format!("file_{i}.md"),
                &format!("description: File {i}\ntype: project"),
            );
        }

        let result = find_relevant_memories("test query", dir, &[], &HashSet::new(), None).await;

        assert_eq!(result.len(), MAX_SELECTED_MEMORIES);
    }
}
