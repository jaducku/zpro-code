pub mod paths;
pub mod scan;
pub mod truncation;
pub mod types;

pub use paths::{get_auto_mem_path, is_auto_mem_path, is_auto_memory_enabled};
pub use scan::{format_memory_manifest, scan_memory_files};
pub use truncation::truncate_entrypoint_content;
pub use types::*;

use std::path::Path;

/// Load memory prompt for system prompt injection.
pub fn load_memory_prompt(cwd: &Path) -> Option<String> {
    if !is_auto_memory_enabled() {
        return None;
    }
    let mem_dir = get_auto_mem_path(cwd)?;
    let memory_md = mem_dir.join("MEMORY.md");
    let content = std::fs::read_to_string(&memory_md).ok()?;
    if content.trim().is_empty() {
        return None;
    }
    let truncated = truncate_entrypoint_content(&content);
    Some(truncated.content)
}

/// Ensure the memory directory exists (idempotent).
pub fn ensure_memory_dir(cwd: &Path) -> Option<std::path::PathBuf> {
    let mem_dir = get_auto_mem_path(cwd)?;
    std::fs::create_dir_all(&mem_dir).ok()?;
    Some(mem_dir)
}
