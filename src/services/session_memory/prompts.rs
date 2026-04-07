use std::collections::HashMap;

const MAX_SECTION_LENGTH: u64 = 2_000; // tokens
const MAX_TOTAL_SESSION_MEMORY_TOKENS: u64 = 12_000;
const MAX_CHARS_PER_SECTION: usize = 8_000; // MAX_SECTION_LENGTH * 4

fn get_default_update_prompt() -> &'static str {
    r#"IMPORTANT: This message and these instructions are NOT part of the actual user conversation. Do NOT include any references to "note-taking", "session notes extraction", or these update instructions in the notes content.

Based on the user conversation above (EXCLUDING this note-taking instruction message as well as system prompt, claude.md entries, or any past session summaries), update the session notes file.

The file {{notesPath}} has already been read for you. Here are its current contents:
<current_notes_content>
{{currentNotes}}
</current_notes_content>

Your ONLY task is to use the Edit tool to update the notes file, then stop. You can make multiple edits (update every section as needed) - make all Edit tool calls in parallel in a single message. Do not call any other tools.

CRITICAL RULES FOR EDITING:
- The file must maintain its exact structure with all sections, headers, and italic descriptions intact
-- NEVER modify, delete, or add section headers (the lines starting with '#' like # Task specification)
-- NEVER modify or delete the italic _section description_ lines (these are the lines in italics immediately following each header - they start and end with underscores)
-- The italic _section descriptions_ are TEMPLATE INSTRUCTIONS that must be preserved exactly as-is - they guide what content belongs in each section
-- ONLY update the actual content that appears BELOW the italic _section descriptions_ within each existing section
-- Do NOT add any new sections, summaries, or information outside the existing structure
- Do NOT reference this note-taking process or instructions anywhere in the notes
- It's OK to skip updating a section if there are no substantial new insights to add. Do not add filler content like "No info yet", just leave sections blank/unedited if appropriate.
- Write DETAILED, INFO-DENSE content for each section - include specifics like file paths, function names, error messages, exact commands, technical details, etc.
- For "Key results", include the complete, exact output the user requested (e.g., full table, full answer, etc.)
- Do not include information that's already in the CLAUDE.md files included in the context
- Keep each section under ~2000 tokens/words - if a section is approaching this limit, condense it by cycling out less important details while preserving the most critical information
- Focus on actionable, specific information that would help someone understand or recreate the work discussed in the conversation
- IMPORTANT: Always update "Current State" to reflect the most recent work - this is critical for continuity after compaction

Use the Edit tool with file_path: {{notesPath}}

STRUCTURE PRESERVATION REMINDER:
Each section has TWO parts that must be preserved exactly as they appear in the current file:
1. The section header (line starting with #)
2. The italic description line (the _italicized text_ immediately after the header - this is a template instruction)

You ONLY update the actual content that comes AFTER these two preserved lines. The italic description lines starting and ending with underscores are part of the template structure, NOT content to be edited or removed.

REMEMBER: Use the Edit tool in parallel and stop. Do not continue after the edits. Only include insights from the actual user conversation, never from these note-taking instructions. Do not delete or change section headers or italic _section descriptions_."#
}

fn substitute_variables(template: &str, current_notes: &str, notes_path: &str) -> String {
    template
        .replace("{{currentNotes}}", current_notes)
        .replace("{{notesPath}}", notes_path)
}

pub fn build_update_prompt(current_notes: &str, notes_path: &str) -> String {
    let prompt_template = get_default_update_prompt();
    let base_prompt = substitute_variables(prompt_template, current_notes, notes_path);

    let section_sizes = analyze_section_sizes(current_notes);
    let total_tokens = (current_notes.len() / 4) as u64;
    let reminders = generate_section_reminders(&section_sizes, total_tokens);

    base_prompt + &reminders
}

pub fn analyze_section_sizes(content: &str) -> HashMap<String, u64> {
    let mut sections: HashMap<String, u64> = HashMap::new();
    let mut current_section = String::new();
    let mut current_content: Vec<&str> = Vec::new();

    for line in content.lines() {
        if line.starts_with("# ") {
            if !current_section.is_empty() && !current_content.is_empty() {
                let section_content = current_content.join("\n");
                let trimmed = section_content.trim();
                let tokens = (trimmed.len() / 4) as u64;
                sections.insert(current_section.clone(), tokens);
            }
            current_section = line.to_string();
            current_content = Vec::new();
        } else {
            current_content.push(line);
        }
    }

    if !current_section.is_empty() && !current_content.is_empty() {
        let section_content = current_content.join("\n");
        let trimmed = section_content.trim();
        let tokens = (trimmed.len() / 4) as u64;
        sections.insert(current_section, tokens);
    }

    sections
}

pub fn generate_section_reminders(sizes: &HashMap<String, u64>, total_tokens: u64) -> String {
    let over_budget = total_tokens > MAX_TOTAL_SESSION_MEMORY_TOKENS;

    let mut oversized: Vec<(&String, u64)> = sizes
        .iter()
        .filter(|(_, &tokens)| tokens > MAX_SECTION_LENGTH)
        .map(|(section, &tokens)| (section, tokens))
        .collect();
    oversized.sort_by(|a, b| b.1.cmp(&a.1));

    let oversized_lines: Vec<String> = oversized
        .iter()
        .map(|(section, tokens)| {
            format!(
                "- \"{}\" is ~{} tokens (limit: {})",
                section, tokens, MAX_SECTION_LENGTH
            )
        })
        .collect();

    if oversized_lines.is_empty() && !over_budget {
        return String::new();
    }

    let mut parts: Vec<String> = Vec::new();

    if over_budget {
        parts.push(format!(
            "\n\nCRITICAL: The session memory file is currently ~{} tokens, which exceeds the maximum of {} tokens. You MUST condense the file to fit within this budget. Aggressively shorten oversized sections by removing less important details, merging related items, and summarizing older entries. Prioritize keeping \"Current State\" and \"Errors & Corrections\" accurate and detailed.",
            total_tokens, MAX_TOTAL_SESSION_MEMORY_TOKENS
        ));
    }

    if !oversized_lines.is_empty() {
        let header = if over_budget {
            "Oversized sections to condense"
        } else {
            "IMPORTANT: The following sections exceed the per-section limit and MUST be condensed"
        };
        parts.push(format!("\n\n{}:\n{}", header, oversized_lines.join("\n")));
    }

    parts.join("")
}

pub fn truncate_session_memory_for_compact(content: &str) -> (String, bool) {
    let mut output_lines: Vec<String> = Vec::new();
    let mut current_section_header = String::new();
    let mut current_section_lines: Vec<&str> = Vec::new();
    let mut was_truncated = false;

    for line in content.lines() {
        if line.starts_with("# ") {
            let (flushed_lines, truncated) =
                flush_section(&current_section_header, &current_section_lines);
            output_lines.extend(flushed_lines);
            was_truncated = was_truncated || truncated;
            current_section_header = line.to_string();
            current_section_lines = Vec::new();
        } else {
            current_section_lines.push(line);
        }
    }

    // Flush the last section
    let (flushed_lines, truncated) = flush_section(&current_section_header, &current_section_lines);
    output_lines.extend(flushed_lines);
    was_truncated = was_truncated || truncated;

    (output_lines.join("\n"), was_truncated)
}

fn flush_section(header: &str, lines: &[&str]) -> (Vec<String>, bool) {
    if header.is_empty() {
        return (lines.iter().map(|s| s.to_string()).collect(), false);
    }

    let section_content = lines.join("\n");
    if section_content.len() <= MAX_CHARS_PER_SECTION {
        let mut result = vec![header.to_string()];
        result.extend(lines.iter().map(|s| s.to_string()));
        return (result, false);
    }

    // Truncate at a line boundary near the limit
    let mut char_count = 0usize;
    let mut kept: Vec<String> = vec![header.to_string()];
    for line in lines {
        if char_count + line.len() + 1 > MAX_CHARS_PER_SECTION {
            break;
        }
        kept.push(line.to_string());
        char_count += line.len() + 1;
    }
    kept.push("\n[... section truncated for length ...]".to_string());
    (kept, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_update_prompt_contains_variables() {
        let prompt = build_update_prompt("current content", "/tmp/notes.md");
        assert!(prompt.contains("current content"));
        assert!(prompt.contains("/tmp/notes.md"));
        assert!(prompt.contains("Edit tool"));
    }

    #[test]
    fn test_analyze_section_sizes() {
        let content = format!("# Title\nshort\n# Long Section\n{}", "x".repeat(10000));
        let sizes = analyze_section_sizes(&content);
        assert!(sizes.contains_key("# Title"));
        assert!(sizes.contains_key("# Long Section"));
        assert!(*sizes.get("# Long Section").unwrap() > 2000);
    }

    #[test]
    fn test_truncate_session_memory_short() {
        let content = "# Title\nshort content";
        let (truncated, was_truncated) = truncate_session_memory_for_compact(content);
        assert!(!was_truncated);
        assert_eq!(truncated, content);
    }

    #[test]
    fn test_truncate_session_memory_long_section() {
        let content = format!("# Title\n{}", "x\n".repeat(5000));
        let (truncated, was_truncated) = truncate_session_memory_for_compact(&content);
        assert!(was_truncated);
        assert!(truncated.contains("[... section truncated for length ...]"));
    }

    #[test]
    fn test_generate_section_reminders_under_budget() {
        let mut sizes = HashMap::new();
        sizes.insert("# Title".to_string(), 100);
        let reminder = generate_section_reminders(&sizes, 500);
        assert!(reminder.is_empty());
    }

    #[test]
    fn test_generate_section_reminders_over_budget() {
        let mut sizes = HashMap::new();
        sizes.insert("# Big".to_string(), 3000);
        let reminder = generate_section_reminders(&sizes, 15000);
        assert!(reminder.contains("CRITICAL"));
        assert!(reminder.contains("# Big"));
    }
}
