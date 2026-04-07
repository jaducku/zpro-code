// Aggressive no-tools preamble. The cache-sharing fork path inherits the
// parent's full tool set (required for cache-key match), and on Sonnet 4.6+
// adaptive-thinking models the model sometimes attempts a tool call despite
// the weaker trailer instruction. With maxTurns: 1, a denied tool call means
// no text output → falls through to the streaming fallback. Putting this
// FIRST and making it explicit about rejection consequences prevents the
// wasted turn.
const NO_TOOLS_PREAMBLE: &str = "CRITICAL: Respond with TEXT ONLY. Do NOT call any tools.\n\
\n\
- Do NOT use Read, Bash, Grep, Glob, Edit, Write, or ANY other tool.\n\
- You already have all the context you need in the conversation above.\n\
- Tool calls will be REJECTED and will waste your only turn — you will fail the task.\n\
- Your entire response must be plain text: an <analysis> block followed by a <summary> block.\n\
\n";

const BASE_COMPACT_PROMPT: &str =
    "Your task is to create a detailed summary of the conversation so far, paying close attention \
to the user's explicit requests and your previous actions.\n\
This summary should be thorough in capturing technical details, code patterns, and architectural \
decisions that would be essential for continuing development work without losing context.\n\
\n\
Before providing your final summary, wrap your analysis in <analysis> tags to organize \
your thoughts and ensure you've covered all necessary points. In your analysis process:\n\
\n\
1. Chronologically analyze each message and section of the conversation. For each section \
thoroughly identify:\n\
   - The user's explicit requests and intents\n\
   - Your approach to addressing the user's requests\n\
   - Key decisions, technical concepts and code patterns\n\
   - Specific details like:\n\
     - file names\n\
     - full code snippets\n\
     - function signatures\n\
     - file edits\n\
   - Errors that you ran into and how you fixed them\n\
   - Pay special attention to specific user feedback that you received, especially if the user \
told you to do something differently.\n\
2. Double-check for technical accuracy and completeness, addressing each required element \
thoroughly.\n\
\n\
Your summary should include the following sections:\n\
\n\
1. Primary Request and Intent: Capture all of the user's explicit requests and intents in detail\n\
2. Key Technical Concepts: List all important technical concepts, technologies, and frameworks \
discussed.\n\
3. Files and Code Sections: Enumerate specific files and code sections examined, modified, or \
created. Pay special attention to the most recent messages and include full code snippets where \
applicable and include a summary of why this file read or edit is important.\n\
4. Errors and fixes: List all errors that you ran into, and how you fixed them. Pay special \
attention to specific user feedback that you received, especially if the user told you to do \
something differently.\n\
5. Problem Solving: Document problems solved and any ongoing troubleshooting efforts.\n\
6. All user messages: List ALL user messages that are not tool results. These are critical for \
understanding the users' feedback and changing intent.\n\
7. Pending Tasks: Outline any pending tasks that you have explicitly been asked to work on.\n\
8. Current Work: Describe in detail precisely what was being worked on immediately before this \
summary request, paying special attention to the most recent messages from both user and \
assistant. Include file names and code snippets where applicable.\n\
9. Optional Next Step: List the next step that you will take that is related to the most recent \
work you were doing. IMPORTANT: ensure that this step is DIRECTLY in line with the user's most \
recent explicit requests, and the task you were working on immediately before this summary \
request. If your last task was concluded, then only list next steps if they are explicitly in \
line with the users request. Do not start on tangential requests or really old requests that \
were already completed without confirming with the user first.\n\
                       If there is a next step, include direct quotes from the most recent \
conversation showing exactly what task you were working on and where you left off. This should \
be verbatim to ensure there's no drift in task interpretation.\n\
\n\
Here's an example of how your output should be structured:\n\
\n\
<example>\n\
<analysis>\n\
[Your thought process, ensuring all points are covered thoroughly and accurately]\n\
</analysis>\n\
\n\
<summary>\n\
1. Primary Request and Intent:\n\
   [Detailed description]\n\
\n\
2. Key Technical Concepts:\n\
   - [Concept 1]\n\
   - [Concept 2]\n\
   - [...]\n\
\n\
3. Files and Code Sections:\n\
   - [File Name 1]\n\
      - [Summary of why this file is important]\n\
      - [Summary of the changes made to this file, if any]\n\
      - [Important Code Snippet]\n\
   - [File Name 2]\n\
      - [Important Code Snippet]\n\
   - [...]\n\
\n\
4. Errors and fixes:\n\
    - [Detailed description of error 1]:\n\
      - [How you fixed the error]\n\
      - [User feedback on the error if any]\n\
    - [...]\n\
\n\
5. Problem Solving:\n\
   [Description of solved problems and ongoing troubleshooting]\n\
\n\
6. All user messages: \n\
    - [Detailed non tool use user message]\n\
    - [...]\n\
\n\
7. Pending Tasks:\n\
   - [Task 1]\n\
   - [Task 2]\n\
   - [...]\n\
\n\
8. Current Work:\n\
   [Precise description of current work]\n\
\n\
9. Optional Next Step:\n\
   [Optional Next step to take]\n\
\n\
</summary>\n\
</example>\n\
\n\
Please provide your summary based on the conversation so far, following this structure and \
ensuring precision and thoroughness in your response. \n\
\n\
There may be additional summarization instructions provided in the included context. If so, \
remember to follow these instructions when creating the above summary. Examples of instructions \
include:\n\
<example>\n\
## Compact Instructions\n\
When summarizing the conversation focus on typescript code changes and also remember the \
mistakes you made and how you fixed them.\n\
</example>\n\
\n\
<example>\n\
# Summary instructions\n\
When you are using compact - please focus on test output and code changes. Include file reads \
verbatim.\n\
</example>\n";

const NO_TOOLS_TRAILER: &str =
    "\n\nREMINDER: Do NOT call any tools. Respond with plain text only — \
an <analysis> block followed by a <summary> block. \
Tool calls will be rejected and you will fail the task.";

/// Build the system prompt for full-conversation compaction.
///
/// Concatenates: NO_TOOLS_PREAMBLE + BASE_COMPACT_PROMPT + optional custom
/// instructions + NO_TOOLS_TRAILER.
pub fn get_compact_prompt(custom_instructions: Option<&str>) -> String {
    let mut prompt = format!("{}{}", NO_TOOLS_PREAMBLE, BASE_COMPACT_PROMPT);

    if let Some(instructions) = custom_instructions {
        let trimmed = instructions.trim();
        if !trimmed.is_empty() {
            prompt.push_str(&format!("\n\nAdditional Instructions:\n{trimmed}"));
        }
    }

    prompt.push_str(NO_TOOLS_TRAILER);
    prompt
}

/// Strip the `<analysis>` scratchpad and replace `<summary>` XML tags with a
/// readable header. Collapses excess blank lines and trims the result.
pub fn format_compact_summary(summary: &str) -> String {
    // Strip analysis section — it's a drafting scratchpad that improves
    // summary quality but has no informational value once written.
    let after_analysis = {
        let start = summary.find("<analysis>");
        let end = summary.find("</analysis>");
        match (start, end) {
            (Some(s), Some(e)) => {
                let end_tag_len = "</analysis>".len();
                format!("{}{}", &summary[..s], &summary[e + end_tag_len..])
            }
            _ => summary.to_owned(),
        }
    };

    // Extract and format summary section.
    let formatted = {
        let open = after_analysis.find("<summary>");
        let close = after_analysis.find("</summary>");
        match (open, close) {
            (Some(s), Some(e)) => {
                let content_start = s + "<summary>".len();
                let content = after_analysis[content_start..e].trim();
                format!(
                    "{}Summary:\n{}{}",
                    &after_analysis[..s],
                    content,
                    &after_analysis[e + "</summary>".len()..]
                )
            }
            _ => after_analysis,
        }
    };

    // Collapse runs of 3+ newlines down to 2.
    let collapsed = collapse_blank_lines(&formatted);
    collapsed.trim().to_owned()
}

/// Replace any run of 3 or more consecutive newlines with exactly two.
fn collapse_blank_lines(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut newline_run = 0usize;

    for ch in s.chars() {
        if ch == '\n' {
            newline_run += 1;
            if newline_run <= 2 {
                result.push(ch);
            }
        } else {
            newline_run = 0;
            result.push(ch);
        }
    }

    result
}

/// Build the user-facing message that wraps the formatted compact summary.
///
/// Ports `getCompactUserSummaryMessage` from the TypeScript original verbatim.
pub fn get_compact_user_summary_message(
    summary: &str,
    suppress_follow_up_questions: bool,
    transcript_path: Option<&str>,
    recent_messages_preserved: bool,
) -> String {
    let formatted_summary = format_compact_summary(summary);

    let mut base = format!(
        "This session is being continued from a previous conversation that ran out of context. \
The summary below covers the earlier portion of the conversation.\n\n{formatted_summary}"
    );

    if let Some(path) = transcript_path {
        base.push_str(&format!(
            "\n\nIf you need specific details from before compaction (like exact code snippets, \
error messages, or content you generated), read the full transcript at: {path}"
        ));
    }

    if recent_messages_preserved {
        base.push_str("\n\nRecent messages are preserved verbatim.");
    }

    if suppress_follow_up_questions {
        base.push_str(
            "\nContinue the conversation from where it left off without asking the user any \
further questions. Resume directly — do not acknowledge the summary, do not recap what was \
happening, do not preface with \"I'll continue\" or similar. Pick up the last task as if the \
break never happened.",
        );
    }

    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_compact_prompt_no_custom() {
        let prompt = get_compact_prompt(None);
        assert!(prompt.starts_with("CRITICAL: Respond with TEXT ONLY"));
        assert!(prompt.contains("Primary Request and Intent"));
        assert!(prompt.contains("Optional Next Step"));
        assert!(prompt.ends_with("you will fail the task."));
        assert!(!prompt.contains("Additional Instructions"));
    }

    #[test]
    fn test_get_compact_prompt_with_custom() {
        let prompt = get_compact_prompt(Some("focus on test output"));
        assert!(prompt.contains("Additional Instructions:\nfocus on test output"));
        assert!(prompt.ends_with("you will fail the task."));
    }

    #[test]
    fn test_format_compact_summary_strips_analysis() {
        let raw = "<analysis>\nthinking here\n</analysis>\n\n<summary>\nthe summary\n</summary>";
        let formatted = format_compact_summary(raw);
        assert!(!formatted.contains("<analysis>"));
        assert!(!formatted.contains("thinking here"));
        assert!(formatted.contains("Summary:"));
        assert!(formatted.contains("the summary"));
    }

    #[test]
    fn test_format_compact_summary_no_tags() {
        let raw = "Just plain text summary";
        let formatted = format_compact_summary(raw);
        assert_eq!(formatted, "Just plain text summary");
    }

    #[test]
    fn test_get_compact_user_summary_message_basic() {
        let msg = get_compact_user_summary_message("the summary", false, None, false);
        assert!(msg.contains("continued from a previous conversation"));
        assert!(msg.contains("the summary"));
        assert!(!msg.contains("without asking"));
    }

    #[test]
    fn test_get_compact_user_summary_message_suppress() {
        let msg = get_compact_user_summary_message("s", true, None, false);
        assert!(msg.contains("without asking the user any further questions"));
    }

    #[test]
    fn test_get_compact_user_summary_message_transcript() {
        let msg = get_compact_user_summary_message("s", false, Some("/tmp/t.json"), false);
        assert!(msg.contains("/tmp/t.json"));
    }

    #[test]
    fn test_get_compact_user_summary_message_preserved() {
        let msg = get_compact_user_summary_message("s", false, None, true);
        assert!(msg.contains("Recent messages are preserved verbatim"));
    }
}
