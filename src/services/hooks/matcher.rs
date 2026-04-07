use regex::Regex;

/// Check if `query` matches the given pattern.
///
/// - `None` → matches everything
/// - `Some("*")` → matches everything
/// - If the pattern contains only alphanumeric, underscore, and pipe chars → pipe-separated exact match
/// - Otherwise → regex match
pub fn matches_pattern(matcher: &Option<String>, query: &str) -> bool {
    let pattern = match matcher {
        None => return true,
        Some(p) => p.as_str(),
    };

    if pattern == "*" {
        return true;
    }

    // Check if pattern is simple: only alphanumeric, underscore, pipe
    let is_simple = pattern
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '|');

    if is_simple {
        pattern.split('|').any(|part| part == query)
    } else {
        Regex::new(pattern)
            .map(|re| re.is_match(query))
            .unwrap_or(false)
    }
}

/// Parse an if-condition string like `"Bash(git *)"` into `("Bash", Some("git *"))`.
///
/// - `"Bash(git *)"` → `Some(("Bash", Some("git *")))`
/// - `"Bash"` → `Some(("Bash", None))`
/// - Missing closing paren → `None`
pub fn parse_if_condition(condition: &str) -> Option<(String, Option<String>)> {
    if let Some(paren_start) = condition.find('(') {
        if !condition.ends_with(')') {
            return None;
        }
        let name = condition[..paren_start].to_string();
        let inner = condition[paren_start + 1..condition.len() - 1].to_string();
        Some((name, Some(inner)))
    } else {
        Some((condition.to_string(), None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_pattern_none_matches_all() {
        assert!(matches_pattern(&None, "anything"));
        assert!(matches_pattern(&None, ""));
    }

    #[test]
    fn test_matches_pattern_star_matches_all() {
        assert!(matches_pattern(&Some("*".to_string()), "Bash"));
        assert!(matches_pattern(&Some("*".to_string()), "Edit"));
    }

    #[test]
    fn test_matches_pattern_exact() {
        assert!(matches_pattern(&Some("Bash".to_string()), "Bash"));
        assert!(!matches_pattern(&Some("Bash".to_string()), "Edit"));
    }

    #[test]
    fn test_matches_pattern_pipe_separated() {
        let pattern = Some("Bash|Edit|Read".to_string());
        assert!(matches_pattern(&pattern, "Bash"));
        assert!(matches_pattern(&pattern, "Edit"));
        assert!(matches_pattern(&pattern, "Read"));
        assert!(!matches_pattern(&pattern, "Write"));
    }

    #[test]
    fn test_matches_pattern_regex() {
        let pattern = Some("Bash.*".to_string());
        assert!(matches_pattern(&pattern, "Bash"));
        assert!(matches_pattern(&pattern, "BashTool"));
        assert!(!matches_pattern(&pattern, "Edit"));
    }

    #[test]
    fn test_parse_if_condition_with_pattern() {
        let result = parse_if_condition("Bash(git *)");
        assert_eq!(result, Some(("Bash".to_string(), Some("git *".to_string()))));
    }

    #[test]
    fn test_parse_if_condition_without_pattern() {
        let result = parse_if_condition("Bash");
        assert_eq!(result, Some(("Bash".to_string(), None)));
    }

    #[test]
    fn test_parse_if_condition_missing_closing_paren() {
        let result = parse_if_condition("Bash(git *");
        assert_eq!(result, None);
    }
}
