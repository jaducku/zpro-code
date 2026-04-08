use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct OutputStyleConfig {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub source: String,
    pub keep_coding_instructions: bool,
    pub force_for_plugin: bool,
}

/// Parse an output style from markdown content with optional YAML frontmatter.
///
/// Frontmatter is delimited by `---` lines. Supported fields:
/// - `name` (defaults to filename without .md)
/// - `description`
/// - `keep-coding-instructions` (bool)
/// - `force-for-plugin` (bool)
///
/// Everything after frontmatter becomes the prompt.
pub fn parse_output_style(content: &str, filename: &str, source: &str) -> OutputStyleConfig {
    let default_name = filename.strip_suffix(".md").unwrap_or(filename).to_string();

    let (frontmatter, body) = split_frontmatter(content);

    let mut name = default_name;
    let mut description = String::new();
    let mut keep_coding_instructions = false;
    let mut force_for_plugin = false;

    if let Some(fm) = frontmatter {
        if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&fm) {
            if let Some(n) = yaml.get("name").and_then(|v| v.as_str()) {
                name = n.to_string();
            }
            if let Some(d) = yaml.get("description").and_then(|v| v.as_str()) {
                description = d.to_string();
            }
            if let Some(b) = yaml
                .get("keep-coding-instructions")
                .and_then(|v| v.as_bool())
            {
                keep_coding_instructions = b;
            }
            if let Some(b) = yaml.get("force-for-plugin").and_then(|v| v.as_bool()) {
                force_for_plugin = b;
            }
        }
    }

    OutputStyleConfig {
        name,
        description,
        prompt: body.trim().to_string(),
        source: source.to_string(),
        keep_coding_instructions,
        force_for_plugin,
    }
}

/// Split content into (Option<frontmatter>, body).
fn split_frontmatter(content: &str) -> (Option<String>, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (None, content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);

    if let Some(end) = after_first.find("\n---") {
        let fm = after_first[..end].to_string();
        let body = &after_first[end + 4..]; // skip "\n---"
        let body = body.strip_prefix('\n').unwrap_or(body);
        (Some(fm), body.to_string())
    } else {
        // No closing delimiter: treat entire content as body (no frontmatter)
        (None, content.to_string())
    }
}

/// Load all .md output style files from a directory.
pub fn load_styles_from_dir(dir: &Path, source: &str) -> Vec<OutputStyleConfig> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut styles = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(content) = fs::read_to_string(&path) {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                styles.push(parse_output_style(&content, filename, source));
            }
        }
    }
    styles
}

/// Return the built-in output styles.
pub fn get_builtin_styles() -> Vec<OutputStyleConfig> {
    vec![
        OutputStyleConfig {
            name: "default".to_string(),
            description: "Default output style".to_string(),
            prompt: String::new(),
            source: "built-in".to_string(),
            keep_coding_instructions: false,
            force_for_plugin: false,
        },
        OutputStyleConfig {
            name: "Explanatory".to_string(),
            description: "Detailed explanations with reasoning".to_string(),
            prompt: "Provide detailed explanations for your changes. Explain your reasoning, the alternatives you considered, and why you chose this approach.".to_string(),
            source: "built-in".to_string(),
            keep_coding_instructions: true,
            force_for_plugin: false,
        },
        OutputStyleConfig {
            name: "Learning".to_string(),
            description: "Educational style with concepts and tips".to_string(),
            prompt: "Adopt a teaching style. Explain concepts as you go, highlight learning opportunities, and suggest further reading or exploration when relevant.".to_string(),
            source: "built-in".to_string(),
            keep_coding_instructions: true,
            force_for_plugin: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_output_style_file() {
        let content = r#"---
name: Concise
description: Short and sweet
keep-coding-instructions: true
force-for-plugin: false
---
Be concise. No fluff."#;

        let style = parse_output_style(content, "concise.md", "userSettings");
        assert_eq!(style.name, "Concise");
        assert_eq!(style.description, "Short and sweet");
        assert_eq!(style.prompt, "Be concise. No fluff.");
        assert_eq!(style.source, "userSettings");
        assert!(style.keep_coding_instructions);
        assert!(!style.force_for_plugin);
    }

    #[test]
    fn test_parse_output_style_no_frontmatter() {
        let content = "Just be helpful.";
        let style = parse_output_style(content, "helpful.md", "built-in");
        assert_eq!(style.name, "helpful");
        assert_eq!(style.description, "");
        assert_eq!(style.prompt, "Just be helpful.");
        assert_eq!(style.source, "built-in");
        assert!(!style.keep_coding_instructions);
        assert!(!style.force_for_plugin);
    }

    #[test]
    fn test_load_styles_from_dir() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("verbose.md"),
            "---\nname: Verbose\ndescription: Talks a lot\n---\nBe verbose.",
        )
        .unwrap();
        std::fs::write(dir.path().join("quiet.md"), "Say less.").unwrap();
        // Non-md file should be ignored
        std::fs::write(dir.path().join("notes.txt"), "ignored").unwrap();

        let styles = load_styles_from_dir(dir.path(), "userSettings");
        assert_eq!(styles.len(), 2);

        let names: Vec<&str> = styles.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Verbose"));
        assert!(names.contains(&"quiet"));
    }

    #[test]
    fn test_load_styles_empty_dir() {
        let dir = TempDir::new().unwrap();
        let styles = load_styles_from_dir(dir.path(), "built-in");
        assert!(styles.is_empty());
    }

    #[test]
    fn test_builtin_styles() {
        let styles = get_builtin_styles();
        assert_eq!(styles.len(), 3);

        let names: Vec<&str> = styles.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"Explanatory"));
        assert!(names.contains(&"Learning"));

        // default has empty prompt
        let default = styles.iter().find(|s| s.name == "default").unwrap();
        assert!(default.prompt.is_empty());
    }
}
