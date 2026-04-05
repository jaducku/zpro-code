use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::commands::context::CommandContext;
use crate::commands::traits::{Command, PromptCommand};
use crate::commands::types::{CommandLoadedFrom, CommandMeta, CommandSource};
use crate::types::content::ContentBlock;

/// Load skills from all discovery locations.
///
/// Discovery order (first match wins when names collide):
/// 1. Project-level: `.agents/skills/` walking from `cwd` up to the git root.
/// 2. User-level: `~/.zpro-code/skills/`.
pub fn load_skills(cwd: &Path) -> Vec<Command> {
    let mut skills = Vec::new();

    // Project-level: walk from cwd upward until we hit a .git or run out of parents.
    let mut dir = cwd.to_path_buf();
    loop {
        let skills_dir = dir.join(".agents/skills");
        if skills_dir.is_dir() {
            skills.extend(load_skills_from_dir(
                &skills_dir,
                CommandSource::ProjectSettings,
            ));
        }
        if dir.join(".git").exists() || !dir.pop() {
            break;
        }
    }

    // User-level: ~/.zpro-code/skills/
    {
        let user_skills = crate::utils::paths::user_skills_dir();
        skills.extend(load_skills_from_dir(&user_skills, CommandSource::UserSettings));
    }

    skills
}

/// Load skills from a single skills directory.
/// Each subdirectory containing a `SKILL.md` file becomes a skill.
fn load_skills_from_dir(dir: &Path, source: CommandSource) -> Vec<Command> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let skill_file = e.path().join("SKILL.md");
            let content = std::fs::read_to_string(&skill_file).ok()?;
            let name = e.file_name().to_string_lossy().to_string();
            parse_skill(&name, &content, &e.path(), source)
        })
        .collect()
}

/// Parse SKILL.md content into a `Command`.
fn parse_skill(
    name: &str,
    content: &str,
    base_dir: &Path,
    source: CommandSource,
) -> Option<Command> {
    let (frontmatter, body) = split_frontmatter(content)?;

    // Parse YAML frontmatter (empty string produces a Null value, which is fine).
    let fm: serde_yaml::Value = serde_yaml::from_str(&frontmatter).ok()?;

    let description = fm
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or(name);

    let when_to_use = fm
        .get("when_to_use")
        .or_else(|| fm.get("whenToUse"))
        .and_then(|v| v.as_str());

    let user_invocable = fm
        .get("user_invocable")
        .or_else(|| fm.get("userInvocable"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Leak strings to satisfy CommandMeta's &'static str fields.
    // Skills are loaded once at startup and live for the process lifetime.
    let leaked_name: &'static str = Box::leak(name.to_string().into_boxed_str());
    let leaked_desc: &'static str = Box::leak(description.to_string().into_boxed_str());
    let leaked_when: Option<&'static str> =
        when_to_use.map(|s| &*Box::leak(s.to_string().into_boxed_str()));

    let meta = CommandMeta {
        name: leaked_name,
        description: leaked_desc,
        source,
        loaded_from: Some(CommandLoadedFrom::Skills),
        when_to_use: leaked_when,
        is_hidden: !user_invocable,
        has_user_specified_description: true,
        user_invocable: Some(user_invocable),
        ..CommandMeta::default()
    };

    Some(Command::Prompt(Box::new(DynamicSkill {
        meta,
        body,
        base_dir: base_dir.to_path_buf(),
    })))
}

/// Split `---\nfrontmatter\n---\nbody` into `(frontmatter, body)`.
///
/// If there is no frontmatter delimiter the entire content is treated as the body.
fn split_frontmatter(content: &str) -> Option<(String, String)> {
    let content = content.trim();
    if !content.starts_with("---") {
        return Some((String::new(), content.to_string()));
    }
    let after_first = &content[3..];
    let end = after_first.find("---")?;
    let frontmatter = after_first[..end].trim().to_string();
    let body = after_first[end + 3..].trim().to_string();
    Some((frontmatter, body))
}

// ---------------------------------------------------------------------------
// DynamicSkill — a skill loaded from a SKILL.md file
// ---------------------------------------------------------------------------

struct DynamicSkill {
    meta: CommandMeta,
    body: String,
    base_dir: PathBuf,
}

#[async_trait]
impl PromptCommand for DynamicSkill {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn progress_message(&self) -> &str {
        "running skill"
    }

    async fn get_prompt(
        &self,
        args: &str,
        _ctx: &CommandContext,
    ) -> anyhow::Result<Vec<ContentBlock>> {
        let mut content = format!(
            "Base directory for this skill: {}\n\n{}",
            self.base_dir.display(),
            self.body
        );

        if !args.is_empty() {
            let arg_parts: Vec<&str> = args.split_whitespace().collect();
            for (i, arg) in arg_parts.iter().enumerate() {
                content = content.replace(&format!("${}", i + 1), arg);
            }
            content = content.replace("$ARGS", args);
        }

        Ok(vec![ContentBlock::Text { text: content }])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_frontmatter_with_yaml() {
        let content = "---\nname: test\ndescription: A test skill\n---\nHello world";
        let (fm, body) = split_frontmatter(content).unwrap();
        assert!(fm.contains("name: test"));
        assert_eq!(body, "Hello world");
    }

    #[test]
    fn test_split_frontmatter_no_frontmatter() {
        let (fm, body) = split_frontmatter("Just content").unwrap();
        assert!(fm.is_empty());
        assert_eq!(body, "Just content");
    }

    #[test]
    fn test_split_frontmatter_empty() {
        let (fm, body) = split_frontmatter("").unwrap();
        assert!(fm.is_empty());
        assert!(body.is_empty());
    }

    #[test]
    fn test_split_frontmatter_unclosed_returns_none() {
        // Only one --- delimiter, no closing one.
        let result = split_frontmatter("---\nname: test\nno closing");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_skill_basic() {
        let content = "---\ndescription: Test skill\n---\nDo something";
        let cmd = parse_skill("my-skill", content, Path::new("/tmp"), CommandSource::ProjectSettings);
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        assert_eq!(cmd.name(), "my-skill");
        assert_eq!(cmd.meta().description, "Test skill");
        assert_eq!(cmd.meta().source, CommandSource::ProjectSettings);
        assert_eq!(cmd.meta().loaded_from, Some(CommandLoadedFrom::Skills));
        assert!(cmd.meta().has_user_specified_description);
    }

    #[test]
    fn test_parse_skill_no_frontmatter() {
        let content = "Just do the thing";
        let cmd = parse_skill("bare-skill", content, Path::new("/tmp"), CommandSource::UserSettings);
        assert!(cmd.is_some());
        let cmd = cmd.unwrap();
        assert_eq!(cmd.name(), "bare-skill");
        // Description falls back to name when no frontmatter.
        assert_eq!(cmd.meta().description, "bare-skill");
    }

    #[test]
    fn test_parse_skill_hidden() {
        let content = "---\ndescription: Hidden\nuser_invocable: false\n---\nSecret";
        let cmd = parse_skill("hidden", content, Path::new("/tmp"), CommandSource::ProjectSettings);
        let cmd = cmd.unwrap();
        assert!(cmd.meta().is_hidden);
        assert_eq!(cmd.meta().user_invocable, Some(false));
    }

    #[test]
    fn test_parse_skill_when_to_use() {
        let content = "---\ndescription: WTU\nwhen_to_use: Use when testing\n---\nBody";
        let cmd = parse_skill("wtu", content, Path::new("/tmp"), CommandSource::ProjectSettings);
        let cmd = cmd.unwrap();
        assert_eq!(cmd.meta().when_to_use, Some("Use when testing"));
    }

    #[test]
    fn test_parse_skill_camel_case_keys() {
        let content = "---\ndescription: Camel\nwhenToUse: Use when camel\nuserInvocable: false\n---\nBody";
        let cmd = parse_skill("camel", content, Path::new("/tmp"), CommandSource::ProjectSettings);
        let cmd = cmd.unwrap();
        assert_eq!(cmd.meta().when_to_use, Some("Use when camel"));
        assert!(cmd.meta().is_hidden);
    }

    #[test]
    fn test_load_skills_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let skills = load_skills(dir.path());
        assert!(skills.is_empty());
    }

    #[test]
    fn test_load_skills_from_project_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        // Create a fake .git so the walk stops here.
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let skills_dir = dir.path().join(".agents/skills/my-skill");
        std::fs::create_dir_all(&skills_dir).unwrap();
        std::fs::write(
            skills_dir.join("SKILL.md"),
            "---\ndescription: Test skill\n---\nDo something",
        )
        .unwrap();
        let skills = load_skills(dir.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name(), "my-skill");
        assert_eq!(skills[0].meta().source, CommandSource::ProjectSettings);
    }

    #[test]
    fn test_load_skills_skips_non_dirs() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let skills_dir = dir.path().join(".agents/skills");
        std::fs::create_dir_all(&skills_dir).unwrap();
        // A file (not a directory) should be ignored.
        std::fs::write(skills_dir.join("not-a-dir.md"), "nope").unwrap();
        let skills = load_skills(dir.path());
        assert!(skills.is_empty());
    }

    #[test]
    fn test_load_skills_skips_dir_without_skill_md() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let skills_dir = dir.path().join(".agents/skills/no-skill");
        std::fs::create_dir_all(&skills_dir).unwrap();
        // Directory exists but has no SKILL.md
        std::fs::write(skills_dir.join("README.md"), "not a skill").unwrap();
        let skills = load_skills(dir.path());
        assert!(skills.is_empty());
    }

    fn make_test_ctx() -> CommandContext {
        use std::sync::Arc;
        use tokio::sync::{mpsc, RwLock};
        use tokio_util::sync::CancellationToken;
        use crate::commands::context::{CommandOptions, CommandContext};
        use crate::state::AppState;

        let (msg_tx, _msg_rx) = mpsc::unbounded_channel();
        let (ui_tx, _ui_rx) = mpsc::unbounded_channel();
        CommandContext {
            app_state: Arc::new(RwLock::new(AppState::default())),
            abort_signal: CancellationToken::new(),
            cwd: PathBuf::from("/tmp"),
            message_tx: msg_tx,
            ui_event_tx: ui_tx,
            options: CommandOptions {
                theme: "default".to_string(),
            },
        }
    }

    #[tokio::test]
    async fn test_dynamic_skill_get_prompt_no_args() {
        let skill = DynamicSkill {
            meta: CommandMeta {
                name: "test",
                description: "test",
                ..CommandMeta::default()
            },
            body: "Hello world".to_string(),
            base_dir: PathBuf::from("/tmp/skill"),
        };

        let ctx = make_test_ctx();
        let result = skill.get_prompt("", &ctx).await.unwrap();
        assert_eq!(result.len(), 1);
        match &result[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("Hello world"));
                assert!(text.contains("/tmp/skill"));
            }
            _ => panic!("expected Text block"),
        }
    }

    #[tokio::test]
    async fn test_dynamic_skill_get_prompt_with_args() {
        let skill = DynamicSkill {
            meta: CommandMeta {
                name: "test",
                description: "test",
                ..CommandMeta::default()
            },
            body: "Fix $1 in $2. Full: $ARGS".to_string(),
            base_dir: PathBuf::from("/tmp/skill"),
        };

        let ctx = make_test_ctx();
        let result = skill.get_prompt("bug.rs main.rs", &ctx).await.unwrap();
        match &result[0] {
            ContentBlock::Text { text } => {
                assert!(text.contains("Fix bug.rs in main.rs"));
                assert!(text.contains("Full: bug.rs main.rs"));
            }
            _ => panic!("expected Text block"),
        }
    }
}
