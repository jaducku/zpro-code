use std::path::Path;

use super::types::MemoryHeader;

const MAX_MEMORY_FILES: usize = 200;
const FRONTMATTER_MAX_LINES: usize = 30;

/// Scan a memory directory for `.md` files (excluding `MEMORY.md`).
///
/// Returns headers sorted by mtime descending (newest first), truncated to 200 entries.
pub fn scan_memory_files(memory_dir: &Path) -> Vec<MemoryHeader> {
    let entries = match std::fs::read_dir(memory_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut headers: Vec<MemoryHeader> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".md") && name != "MEMORY.md"
        })
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let mtime_ms = metadata
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis() as u64;

            let content = std::fs::read_to_string(entry.path()).ok()?;
            let (description, memory_type) = parse_frontmatter(&content);

            Some(MemoryHeader {
                filename: entry.file_name().to_string_lossy().into_owned(),
                file_path: entry.path(),
                mtime_ms,
                description,
                memory_type,
            })
        })
        .collect();

    headers.sort_by(|a, b| b.mtime_ms.cmp(&a.mtime_ms));
    headers.truncate(MAX_MEMORY_FILES);
    headers
}

/// Parse YAML frontmatter from markdown content.
///
/// Looks for `---` delimiters in the first 30 lines and extracts
/// `description:` and `type:` values.
pub fn parse_frontmatter(content: &str) -> (Option<String>, Option<String>) {
    let lines: Vec<&str> = content.lines().take(FRONTMATTER_MAX_LINES).collect();

    if lines.is_empty() || lines[0].trim() != "---" {
        return (None, None);
    }

    let end = lines[1..]
        .iter()
        .position(|line| line.trim() == "---")
        .map(|pos| pos + 1); // offset because we started from index 1

    let end = match end {
        Some(end) => end,
        None => return (None, None),
    };

    let mut description = None;
    let mut memory_type = None;

    for line in &lines[1..end] {
        if let Some(value) = line.strip_prefix("description:") {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                description = Some(value.to_string());
            }
        } else if let Some(value) = line.strip_prefix("type:") {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                memory_type = Some(value.to_string());
            }
        }
    }

    (description, memory_type)
}

/// Format memory headers into a manifest string.
///
/// Each line: `[type] filename : description`
pub fn format_memory_manifest(memories: &[MemoryHeader]) -> String {
    memories
        .iter()
        .map(|m| {
            let type_tag = m.memory_type.as_deref().unwrap_or("unknown");
            let desc = m.description.as_deref().unwrap_or("");
            format!("[{}] {} : {}", type_tag, m.filename, desc)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_scan_memory_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // MEMORY.md should be excluded
        std::fs::write(dir.join("MEMORY.md"), "# Main memory").unwrap();

        // Two topic files
        std::fs::write(
            dir.join("project_setup.md"),
            "---\ndescription: Setup notes\ntype: project\n---\nContent here",
        )
        .unwrap();
        std::fs::write(
            dir.join("feedback_review.md"),
            "---\ndescription: Review feedback\ntype: feedback\n---\nMore content",
        )
        .unwrap();

        let results = scan_memory_files(dir);
        assert_eq!(results.len(), 2);
        // MEMORY.md excluded
        assert!(results.iter().all(|h| h.filename != "MEMORY.md"));
    }

    #[test]
    fn test_scan_memory_files_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let results = scan_memory_files(tmp.path());
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_frontmatter() {
        let content = "---\ndescription: My project notes\ntype: project\n---\n# Hello";
        let (desc, mtype) = parse_frontmatter(content);
        assert_eq!(desc.as_deref(), Some("My project notes"));
        assert_eq!(mtype.as_deref(), Some("project"));
    }

    #[test]
    fn test_parse_frontmatter_no_delimiters() {
        let content = "# Just a heading\nSome content here.";
        let (desc, mtype) = parse_frontmatter(content);
        assert!(desc.is_none());
        assert!(mtype.is_none());
    }

    #[test]
    fn test_format_memory_manifest() {
        let memories = vec![
            MemoryHeader {
                filename: "setup.md".to_string(),
                file_path: PathBuf::from("/tmp/setup.md"),
                mtime_ms: 1000,
                description: Some("Project setup".to_string()),
                memory_type: Some("project".to_string()),
            },
            MemoryHeader {
                filename: "review.md".to_string(),
                file_path: PathBuf::from("/tmp/review.md"),
                mtime_ms: 900,
                description: Some("Code review notes".to_string()),
                memory_type: Some("feedback".to_string()),
            },
        ];

        let result = format_memory_manifest(&memories);
        assert_eq!(
            result,
            "[project] setup.md : Project setup\n[feedback] review.md : Code review notes"
        );
    }
}
