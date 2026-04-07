pub const SCORE_MATCH: i32 = 16;
pub const BONUS_BOUNDARY: i32 = 8;
pub const BONUS_CAMEL: i32 = 6;
pub const BONUS_CONSECUTIVE: i32 = 4;
pub const BONUS_FIRST_CHAR: i32 = 8;
pub const PENALTY_GAP_START: i32 = 3;
pub const PENALTY_GAP_EXTENSION: i32 = 1;

/// Returns true if the byte is a path/word boundary: `/`, `\`, `-`, `_`, `.`, or space.
pub fn is_boundary(b: u8) -> bool {
    matches!(b, b'/' | b'\\' | b'-' | b'_' | b'.' | b' ')
}

/// Returns the positional bonus for a match at `pos` in `path`.
///
/// - pos == 0 -> BONUS_FIRST_CHAR
/// - previous byte is a boundary -> BONUS_BOUNDARY
/// - CamelCase transition (prev lowercase, current uppercase) -> BONUS_CAMEL
/// - Otherwise -> 0
pub fn score_bonus_at(path: &[u8], pos: usize) -> i32 {
    if pos == 0 {
        return BONUS_FIRST_CHAR;
    }
    let prev = path[pos - 1];
    if is_boundary(prev) {
        return BONUS_BOUNDARY;
    }
    let cur = path[pos];
    if prev.is_ascii_lowercase() && cur.is_ascii_uppercase() {
        return BONUS_CAMEL;
    }
    0
}

/// Compute a 26-bit bitmap for the lowercase a-z characters present in `s`.
pub fn compute_char_bitmap(s: &str) -> u32 {
    let mut bits: u32 = 0;
    for &b in s.as_bytes() {
        let lower = b.to_ascii_lowercase();
        if lower.is_ascii_lowercase() {
            bits |= 1 << (lower - b'a');
        }
    }
    bits
}

/// Returns true if the path bitmap contains all characters present in the query.
pub fn bitmap_contains_query(path_bits: u32, query: &str) -> bool {
    let query_bits = compute_char_bitmap(query);
    (path_bits & query_bits) == query_bits
}

/// Returns true if the query contains any uppercase ASCII character.
pub fn is_case_sensitive(query: &str) -> bool {
    query.bytes().any(|b| b.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_boundary() {
        assert!(is_boundary(b'/'));
        assert!(is_boundary(b'\\'));
        assert!(is_boundary(b'-'));
        assert!(is_boundary(b'_'));
        assert!(is_boundary(b'.'));
        assert!(is_boundary(b' '));
        assert!(!is_boundary(b'a'));
    }

    #[test]
    fn test_score_bonus_at_first_char() {
        assert_eq!(score_bonus_at(b"hello", 0), BONUS_FIRST_CHAR);
    }

    #[test]
    fn test_score_bonus_at_boundary() {
        assert_eq!(score_bonus_at(b"src/main.rs", 4), BONUS_BOUNDARY);
    }

    #[test]
    fn test_score_bonus_at_camel() {
        assert_eq!(score_bonus_at(b"camelCase", 5), BONUS_CAMEL);
    }

    #[test]
    fn test_score_bonus_at_regular() {
        assert_eq!(score_bonus_at(b"hello", 2), 0);
    }

    #[test]
    fn test_compute_char_bitmap() {
        let bits = compute_char_bitmap("hello");
        // h, e, l, o should be set
        assert_ne!(bits & (1 << (b'h' - b'a')), 0);
        assert_ne!(bits & (1 << (b'e' - b'a')), 0);
        assert_ne!(bits & (1 << (b'l' - b'a')), 0);
        assert_ne!(bits & (1 << (b'o' - b'a')), 0);
        // z should not be set
        assert_eq!(bits & (1 << (b'z' - b'a')), 0);
    }

    #[test]
    fn test_bitmap_rejects_missing_char() {
        let bits = compute_char_bitmap("hello");
        assert!(!bitmap_contains_query(bits, "hz"));
    }

    #[test]
    fn test_bitmap_accepts_matching() {
        let bits = compute_char_bitmap("hello");
        assert!(bitmap_contains_query(bits, "hel"));
    }

    #[test]
    fn test_smart_case_detection() {
        assert!(!is_case_sensitive("hello"));
        assert!(is_case_sensitive("Hello"));
    }
}
