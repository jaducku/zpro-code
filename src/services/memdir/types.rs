use std::path::PathBuf;

pub const MEMORY_TYPES: &[&str] = &["user", "feedback", "project", "reference"];

pub struct MemoryHeader {
    pub filename: String,
    pub file_path: PathBuf,
    pub mtime_ms: u64,
    pub description: Option<String>,
    pub memory_type: Option<String>,
}

pub struct RelevantMemory {
    pub path: PathBuf,
    pub mtime_ms: u64,
}

pub struct EntrypointTruncation {
    pub content: String,
    pub line_count: usize,
    pub byte_count: usize,
    pub was_line_truncated: bool,
    pub was_byte_truncated: bool,
}
