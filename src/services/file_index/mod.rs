pub mod scoring;

use scoring::*;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub path: String,
    pub score: f64, // 0-1, lower is better
}

pub struct FileIndex {
    paths: Vec<String>,
    lower_paths: Vec<String>,
    char_bits: Vec<u32>,
    path_lens: Vec<u16>,
    top_level_cache: Option<Vec<SearchResult>>,
}

impl FileIndex {
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            lower_paths: Vec::new(),
            char_bits: Vec::new(),
            path_lens: Vec::new(),
            top_level_cache: None,
        }
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn load_from_file_list(&mut self, file_list: Vec<String>) {
        let mut seen = HashSet::new();
        let mut deduped = Vec::new();
        for p in file_list {
            if seen.insert(p.clone()) {
                deduped.push(p);
            }
        }

        self.lower_paths = deduped.iter().map(|p| p.to_ascii_lowercase()).collect();
        self.char_bits = self.lower_paths.iter().map(|p| compute_char_bitmap(p)).collect();
        self.path_lens = deduped
            .iter()
            .map(|p| p.len().min(u16::MAX as usize) as u16)
            .collect();
        self.paths = deduped;
        self.top_level_cache = None;
    }

    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        if query.is_empty() {
            return self.top_level_entries(limit);
        }

        let query = if query.len() > 64 {
            &query[..64]
        } else {
            query
        };

        let case_sensitive = is_case_sensitive(query);
        let needle: Vec<u8> = if case_sensitive {
            query.as_bytes().to_vec()
        } else {
            query.bytes().map(|b| b.to_ascii_lowercase()).collect()
        };

        let mut results: Vec<SearchResult> = Vec::new();

        for i in 0..self.paths.len() {
            if !bitmap_contains_query(self.char_bits[i], query) {
                continue;
            }

            let path_bytes = if case_sensitive {
                self.paths[i].as_bytes()
            } else {
                self.lower_paths[i].as_bytes()
            };

            let score = match self.fuzzy_score(path_bytes, &needle, self.path_lens[i] as usize) {
                Some(s) => s,
                None => continue,
            };

            // Test file penalty
            let score = if self.lower_paths[i].contains("test") {
                (score * 1.05).min(1.0)
            } else {
                score
            };

            // Maintain top-k sorted ascending by score (lower is better)
            let result = SearchResult {
                path: self.paths[i].clone(),
                score,
            };

            if results.len() < limit {
                let pos = results
                    .binary_search_by(|r| r.score.partial_cmp(&score).unwrap())
                    .unwrap_or_else(|p| p);
                results.insert(pos, result);
            } else if let Some(last) = results.last() {
                if score < last.score {
                    let pos = results
                        .binary_search_by(|r| r.score.partial_cmp(&score).unwrap())
                        .unwrap_or_else(|p| p);
                    results.insert(pos, result);
                    results.truncate(limit);
                }
            }
        }

        results
    }

    fn fuzzy_score(&self, path: &[u8], needle: &[u8], path_len: usize) -> Option<f64> {
        if needle.is_empty() {
            return Some(0.0);
        }

        let needle_len = needle.len();

        // Find all start positions: first occurrence of needle[0] + boundary positions
        let mut starts = Vec::new();
        let mut first_found = false;
        for (i, &b) in path.iter().enumerate() {
            if b == needle[0] {
                if !first_found {
                    starts.push(i);
                    first_found = true;
                } else if i == 0 || is_boundary(path[i - 1]) {
                    starts.push(i);
                }
            }
        }

        if starts.is_empty() {
            return None;
        }

        let max_possible = (needle_len as i32) * SCORE_MATCH
            + (needle_len as i32) * BONUS_BOUNDARY
            + ((needle_len as i32) - 1) * BONUS_CONSECUTIVE
            + 32;

        let mut best_normalized: Option<f64> = None;

        for &start in &starts {
            let mut raw_score: i32 = 0;
            let mut prev_match_pos = start;
            let mut matched = true;
            let mut ni = 0; // needle index
            let mut pi = start; // path index

            while ni < needle_len && pi < path.len() {
                if path[pi] == needle[ni] {
                    let gap = if ni == 0 { 0 } else { pi - prev_match_pos - 1 };

                    if ni > 0 {
                        if gap == 0 {
                            raw_score += BONUS_CONSECUTIVE;
                        } else {
                            raw_score -= PENALTY_GAP_START + (gap as i32) * PENALTY_GAP_EXTENSION;
                        }
                    }

                    raw_score += score_bonus_at(path, pi);
                    raw_score += SCORE_MATCH;

                    prev_match_pos = pi;
                    ni += 1;
                }
                pi += 1;
            }

            if ni < needle_len {
                matched = false;
            }

            if !matched {
                continue;
            }

            // Length bonus
            let length_bonus = 0i32.max(32 - (path_len as i32 >> 2));
            raw_score += length_bonus;

            let normalized = 1.0 - ((raw_score as f64) / (max_possible as f64)).clamp(0.0, 1.0);

            best_normalized = Some(match best_normalized {
                Some(prev) => prev.min(normalized),
                None => normalized,
            });
        }

        best_normalized
    }

    fn top_level_entries(&self, limit: usize) -> Vec<SearchResult> {
        let mut seen = HashSet::new();
        let mut segments = Vec::new();

        for path in &self.paths {
            let seg = match path.find(|c| c == '/' || c == '\\') {
                Some(pos) => &path[..pos],
                None => path.as_str(),
            };
            if seen.insert(seg.to_string()) {
                segments.push(seg.to_string());
            }
        }

        segments.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));

        let cap = limit.min(100).min(segments.len());
        segments
            .into_iter()
            .take(cap)
            .map(|s| SearchResult {
                path: s,
                score: 0.0,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_deduplicates() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec!["a".into(), "a".into(), "b".into()]);
        assert_eq!(idx.len(), 2);
    }

    #[test]
    fn test_search_exact_match() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/lib.rs".into(),
            "src/main.rs".into(),
            "src/utils.rs".into(),
            "README.md".into(),
        ]);
        let results = idx.search("main", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].path, "src/main.rs");
    }

    #[test]
    fn test_search_empty_returns_top() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/main.rs".into(),
            "src/lib.rs".into(),
            "docs/readme.md".into(),
        ]);
        let results = idx.search("", 10);
        assert!(!results.is_empty());
        // Should return top-level segments like "src", "docs"
        let paths: Vec<&str> = results.iter().map(|r| r.path.as_str()).collect();
        assert!(paths.contains(&"src"));
        assert!(paths.contains(&"docs"));
    }

    #[test]
    fn test_search_test_file_penalty() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/utils.rs".into(),
            "tests/test_utils.rs".into(),
        ]);
        let results = idx.search("utils", 10);
        assert!(results.len() >= 2);
        // src/utils.rs should rank above tests/test_utils.rs
        let src_pos = results.iter().position(|r| r.path == "src/utils.rs").unwrap();
        let test_pos = results
            .iter()
            .position(|r| r.path == "tests/test_utils.rs")
            .unwrap();
        assert!(
            src_pos < test_pos,
            "src/utils.rs (pos {}) should rank above tests/test_utils.rs (pos {})",
            src_pos,
            test_pos
        );
    }

    #[test]
    fn test_search_respects_limit() {
        let mut idx = FileIndex::new();
        let files: Vec<String> = (0..100).map(|i| format!("src/file_{}.rs", i)).collect();
        idx.load_from_file_list(files);
        let results = idx.search("file", 5);
        assert_eq!(results.len(), 5);
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/MyFile.rs".into(),
            "src/other.rs".into(),
        ]);
        // All-lowercase query → case-insensitive
        let results = idx.search("myfile", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].path, "src/MyFile.rs");
    }

    #[test]
    fn test_search_case_sensitive() {
        let mut idx = FileIndex::new();
        idx.load_from_file_list(vec![
            "src/MyFile.rs".into(),
            "src/myfile.rs".into(),
        ]);
        // Mixed-case query → case-sensitive, should prefer exact match
        let results = idx.search("MyFile", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].path, "src/MyFile.rs");
    }
}
