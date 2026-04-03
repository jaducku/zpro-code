use super::types::EntrypointTruncation;

pub const MAX_ENTRYPOINT_LINES: usize = 200;
pub const MAX_ENTRYPOINT_BYTES: usize = 25_000;

/// Truncate entrypoint content to fit within line and byte limits.
///
/// 1. Trim content
/// 2. If lines > 200: slice to first 200, set was_line_truncated
/// 3. If bytes > 25000: cut at last newline before cap, set was_byte_truncated
pub fn truncate_entrypoint_content(raw: &str) -> EntrypointTruncation {
    let trimmed = raw.trim();

    let lines: Vec<&str> = trimmed.lines().collect();
    let total_lines = lines.len();

    let mut was_line_truncated = false;
    let mut was_byte_truncated = false;

    let after_line_truncation = if total_lines > MAX_ENTRYPOINT_LINES {
        was_line_truncated = true;
        lines[..MAX_ENTRYPOINT_LINES].join("\n")
    } else {
        trimmed.to_string()
    };

    let content = if after_line_truncation.len() > MAX_ENTRYPOINT_BYTES {
        was_byte_truncated = true;
        // Find the last newline before the byte cap
        let slice = &after_line_truncation[..MAX_ENTRYPOINT_BYTES];
        match slice.rfind('\n') {
            Some(pos) => after_line_truncation[..pos].to_string(),
            None => slice.to_string(),
        }
    } else {
        after_line_truncation
    };

    let line_count = content.lines().count();
    let byte_count = content.len();

    EntrypointTruncation {
        content,
        line_count,
        byte_count,
        was_line_truncated,
        was_byte_truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_within_limits() {
        let input = "line 1\nline 2\nline 3";
        let result = truncate_entrypoint_content(input);

        assert_eq!(result.content, "line 1\nline 2\nline 3");
        assert_eq!(result.line_count, 3);
        assert!(!result.was_line_truncated);
        assert!(!result.was_byte_truncated);
    }

    #[test]
    fn test_truncate_exceeds_line_limit() {
        let lines: Vec<String> = (0..250).map(|i| format!("line {}", i)).collect();
        let input = lines.join("\n");

        let result = truncate_entrypoint_content(&input);

        assert!(result.was_line_truncated);
        assert!(!result.was_byte_truncated);
        assert_eq!(result.line_count, MAX_ENTRYPOINT_LINES);
        // Verify content is exactly the first 200 lines
        assert!(result.content.starts_with("line 0\n"));
        assert!(result.content.contains("line 199"));
        assert!(!result.content.contains("line 200\n"));
    }

    #[test]
    fn test_truncate_exceeds_byte_limit() {
        // Each line ~130 bytes, 200 lines = ~26KB > 25KB
        let lines: Vec<String> = (0..200)
            .map(|i| format!("line {:04}: {}", i, "x".repeat(120)))
            .collect();
        let input = lines.join("\n");
        assert!(input.len() > MAX_ENTRYPOINT_BYTES);

        let result = truncate_entrypoint_content(&input);

        assert!(result.was_byte_truncated);
        assert!(result.byte_count <= MAX_ENTRYPOINT_BYTES);
        // Should end at a newline boundary
        assert!(!result.content.ends_with('\n'));
    }

    #[test]
    fn test_truncate_trims_whitespace() {
        let input = "  \n  hello world  \n  ";
        let result = truncate_entrypoint_content(input);
        assert_eq!(result.content, "hello world");
    }
}
