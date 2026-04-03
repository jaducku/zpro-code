use std::path::Path;

use crate::services::api::types::SystemBlock;

/// Configuration for system prompt assembly with skill listing.
pub struct SystemPromptConfig<'a> {
    pub cwd: &'a Path,
    pub custom_prompt: Option<&'a str>,
    pub append_prompt: Option<&'a str>,
    pub output_style: Option<&'a str>,
    pub skill_entries: &'a [(String, String, Option<String>)],
}

const MAX_SKILL_DESC_CHARS: usize = 250;

fn format_skill_listing(entries: &[(String, String, Option<String>)]) -> String {
    entries
        .iter()
        .map(|(name, desc, when_to_use)| {
            let full_desc = match when_to_use {
                Some(wtu) => format!("{} - {}", desc, wtu),
                None => desc.clone(),
            };
            let truncated = if full_desc.len() > MAX_SKILL_DESC_CHARS {
                format!("{}…", &full_desc[..MAX_SKILL_DESC_CHARS - 1])
            } else {
                full_desc
            };
            format!("- {}: {}", name, truncated)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Assemble system prompt with skill listing included.
pub fn assemble_system_prompt_with_config(config: &SystemPromptConfig) -> Vec<SystemBlock> {
    let mut blocks = assemble_system_prompt(
        config.cwd,
        config.custom_prompt,
        config.append_prompt,
        config.output_style,
    );

    if !config.skill_entries.is_empty() {
        let listing = format_skill_listing(config.skill_entries);
        blocks.push(SystemBlock::Text {
            text: format!(
                "<system-reminder>\nThe following skills are available for use with the Skill tool:\n\n{}\n</system-reminder>",
                listing
            ),
        });
    }

    blocks
}

/// Assemble the full system prompt from all sources.
pub fn assemble_system_prompt(
    cwd: &Path,
    custom_prompt: Option<&str>,
    append_prompt: Option<&str>,
    output_style: Option<&str>,
) -> Vec<SystemBlock> {
    let mut blocks = Vec::new();

    // 1. Core instructions
    blocks.push(SystemBlock::Text {
        text: core_instructions().to_string(),
    });

    // 2. AGENTS.md files (project context)
    let agents_content = load_agents_md(cwd);
    if !agents_content.is_empty() {
        blocks.push(SystemBlock::Text {
            text: format!("# Project Instructions\n\n{}", agents_content),
        });
    }

    // 3. Memory (auto-memory from .memory/ dir)
    if let Some(memory_content) = crate::services::memdir::load_memory_prompt(cwd) {
        blocks.push(SystemBlock::Text {
            text: format!("# Memory\n\n{}", memory_content),
        });
    }

    // 3b. Output style
    if let Some(style_name) = output_style {
        if let Some(style_config) = crate::services::output_styles::get_output_style_config(cwd, style_name) {
            blocks.push(SystemBlock::Text {
                text: format!("# Output Style: {}\n\n{}", style_config.name, style_config.prompt),
            });
        }
    }

    // 4. Git context
    let git_context = gather_git_context(cwd);
    if !git_context.is_empty() {
        blocks.push(SystemBlock::Text {
            text: format!("# Environment\n\n{}", git_context),
        });
    }

    // 5. Custom system prompt (--system-prompt overrides core)
    if let Some(custom) = custom_prompt {
        blocks = vec![SystemBlock::Text {
            text: custom.to_string(),
        }];
    }

    // 6. Append prompt (--append-system-prompt adds to end)
    if let Some(append) = append_prompt {
        blocks.push(SystemBlock::Text {
            text: append.to_string(),
        });
    }

    blocks
}

fn core_instructions() -> &'static str {
    r#"You are an AI coding assistant. You help users with software engineering tasks including writing code, debugging, refactoring, and explaining code.

# Guidelines
- Be concise and direct
- Write clean, correct code
- Follow existing code style and conventions
- Only modify what's necessary
- Don't add unnecessary features or abstractions
- Explain your reasoning when making non-obvious choices

# Tools
You have access to tools for reading files, writing files, running commands, and searching code. Use them to understand the codebase before making changes."#
}

/// Discover and load AGENTS.md files.
///
/// Discovery order:
/// 1. `~/.atom/AGENTS.md` (global user)
/// 2. From CWD upward to git root (or filesystem root): `AGENTS.md`, `.agents/AGENTS.md`
/// 3. `AGENTS.local.md` in CWD (private, project-level)
fn load_agents_md(cwd: &Path) -> String {
    let mut contents = Vec::new();

    // 1. Global user AGENTS.md
    {
        let global = crate::utils::paths::user_agents_md_path();
        if let Ok(content) = std::fs::read_to_string(&global) {
            contents.push(format!("## Global Instructions\n{}", content));
        }
    }

    // 2. Walk from CWD upward to find AGENTS.md files
    let mut dir = cwd.to_path_buf();
    let mut project_files = Vec::new();
    loop {
        for name in &["AGENTS.md", ".agents/AGENTS.md"] {
            let path = dir.join(name);
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    project_files.push((path.clone(), content));
                }
            }
        }
        // Stop at git root or filesystem root
        if dir.join(".git").exists() || !dir.pop() {
            break;
        }
    }
    // Reverse so root-level comes first, CWD-level last (higher priority)
    project_files.reverse();
    for (path, content) in project_files {
        let relative = path.strip_prefix(cwd).unwrap_or(&path);
        contents.push(format!("## From {}\n{}", relative.display(), content));
    }

    // 3. Local (private) AGENTS.local.md
    let local = cwd.join("AGENTS.local.md");
    if let Ok(content) = std::fs::read_to_string(&local) {
        contents.push(format!("## Local Instructions\n{}", content));
    }

    contents.join("\n\n")
}

/// Gather git context for the system prompt.
fn gather_git_context(cwd: &Path) -> String {
    // Check if inside a git work tree
    let is_git = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(cwd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !is_git {
        // Still include working directory and date even outside git
        let mut parts = Vec::new();
        parts.push(format!("- Working directory: {}", cwd.display()));
        parts.push(format!("- Date: {}", chrono::Local::now().format("%Y-%m-%d")));
        return parts.join("\n");
    }

    let mut parts = Vec::new();

    // Current branch
    if let Ok(output) = std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(cwd)
        .output()
    {
        if output.status.success() {
            let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !branch.is_empty() {
                parts.push(format!("- Git branch: {}", branch));
            }
        }
    }

    // Recent commits (last 5)
    if let Ok(output) = std::process::Command::new("git")
        .args(["log", "--oneline", "-5"])
        .current_dir(cwd)
        .output()
    {
        if output.status.success() {
            let log = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !log.is_empty() {
                parts.push(format!("- Recent commits:\n{}", log));
            }
        }
    }

    // Working directory and date
    parts.push(format!("- Working directory: {}", cwd.display()));
    parts.push(format!("- Date: {}", chrono::Local::now().format("%Y-%m-%d")));

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_assemble_with_no_agents_md() {
        let dir = TempDir::new().unwrap();
        let blocks = assemble_system_prompt(dir.path(), None, None, None);
        assert!(!blocks.is_empty()); // at least core instructions
    }

    #[test]
    fn test_assemble_with_agents_md() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "Use TypeScript.").unwrap();
        let blocks = assemble_system_prompt(dir.path(), None, None, None);
        let full: String = blocks
            .iter()
            .map(|b| match b {
                SystemBlock::Text { text } => text.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(full.contains("Use TypeScript"));
    }

    #[test]
    fn test_custom_prompt_overrides() {
        let dir = TempDir::new().unwrap();
        let blocks = assemble_system_prompt(dir.path(), Some("Custom only"), None, None);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            SystemBlock::Text { text } => assert_eq!(text, "Custom only"),
        }
    }

    #[test]
    fn test_load_memory_prompt_returns_none_without_git() {
        let dir = TempDir::new().unwrap();
        // No git repo → get_auto_mem_path returns None → load_memory_prompt returns None
        let result = crate::services::memdir::load_memory_prompt(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_append_prompt() {
        let dir = TempDir::new().unwrap();
        let blocks = assemble_system_prompt(dir.path(), None, Some("Extra info"), None);
        let last = blocks.last().unwrap();
        match last {
            SystemBlock::Text { text } => assert!(text.contains("Extra info")),
        }
    }

    #[test]
    fn test_format_skill_listing_basic() {
        let entries = vec![
            (
                "commit".to_string(),
                "Commit changes".to_string(),
                Some("Use after making changes".to_string()),
            ),
            ("review".to_string(), "Review code".to_string(), None),
        ];
        let listing = format_skill_listing(&entries);
        assert!(listing.contains("- commit: Commit changes - Use after making changes"));
        assert!(listing.contains("- review: Review code"));
    }

    #[test]
    fn test_format_skill_listing_truncation() {
        let long_desc = "x".repeat(300);
        let entries = vec![("long".to_string(), long_desc, None)];
        let listing = format_skill_listing(&entries);
        assert!(listing.len() < 300);
        assert!(listing.contains('…'));
    }

    #[test]
    fn test_format_skill_listing_empty() {
        let entries: Vec<(String, String, Option<String>)> = vec![];
        let listing = format_skill_listing(&entries);
        assert!(listing.is_empty());
    }

    #[test]
    fn test_assemble_with_skills() {
        let dir = TempDir::new().unwrap();
        let config = SystemPromptConfig {
            cwd: dir.path(),
            custom_prompt: None,
            append_prompt: None,
            output_style: None,
            skill_entries: &[(
                "brainstorming".to_string(),
                "Creative work".to_string(),
                Some("Use before features".to_string()),
            )],
        };
        let blocks = assemble_system_prompt_with_config(&config);
        let full: String = blocks
            .iter()
            .map(|b| match b {
                SystemBlock::Text { text } => text.as_str(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(full.contains("<system-reminder>"));
        assert!(full.contains("brainstorming"));
        assert!(full.contains("Skill tool"));
    }
}
