use std::sync::LazyLock;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

/// Calculate display width of a string, accounting for CJK double-width characters.
fn display_width(s: &str) -> usize {
    s.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum()
}

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME: LazyLock<Theme> = LazyLock::new(|| {
    let ts = ThemeSet::load_defaults();
    ts.themes["base16-ocean.dark"].clone()
});

pub fn parse_markdown(text: &str) -> Vec<Line<'static>> {
    let mut renderer = MarkdownRenderer::new();
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(text, opts);
    for event in parser {
        renderer.handle_event(event);
    }
    let mut lines = renderer.finish();
    auto_detect_links(&mut lines);
    lines
}

/// Post-process lines to detect URLs and GitHub issue references in unstyled spans.
fn auto_detect_links(lines: &mut [Line<'static>]) {
    let link_style = Style::new().fg(Color::Blue).add_modifier(Modifier::UNDERLINED);

    for line in lines.iter_mut() {
        let mut new_spans: Vec<Span<'static>> = Vec::new();
        for span in std::mem::take(&mut line.spans) {
            // Only process spans with default style (plain text)
            if span.style != Style::default() {
                new_spans.push(span);
                continue;
            }

            let text: &str = &span.content;
            let mut last = 0;
            let mut splits: Vec<(usize, usize, bool)> = Vec::new(); // (start, end, is_link)

            // Find URLs: https://...
            let mut search_from = 0;
            while let Some(pos) = text[search_from..].find("https://") {
                let start = search_from + pos;
                // URL extends until whitespace or end
                let end = text[start..]
                    .find(|c: char| c.is_whitespace() || c == ')' || c == ']' || c == '>' || c == '"' || c == '\'')
                    .map(|e| start + e)
                    .unwrap_or(text.len());
                if end > start + 8 {
                    // At least "https://X"
                    splits.push((start, end, true));
                }
                search_from = end;
            }

            // Find GitHub refs: word/word#digits (e.g. owner/repo#123)
            // Simple scan: look for '#' preceded by word/word
            let bytes = text.as_bytes();
            for (i, &b) in bytes.iter().enumerate() {
                if b != b'#' {
                    continue;
                }
                // Check digits after #
                let digit_start = i + 1;
                if digit_start >= bytes.len() || !bytes[digit_start].is_ascii_digit() {
                    continue;
                }
                let digit_end = bytes[digit_start..]
                    .iter()
                    .position(|b| !b.is_ascii_digit())
                    .map(|p| digit_start + p)
                    .unwrap_or(bytes.len());
                // Check owner/repo before #
                // Walk back to find `/`
                if i == 0 {
                    continue;
                }
                let before = &text[..i];
                if let Some(slash_pos) = before.rfind('/') {
                    // owner = before[..slash_pos], repo = before[slash_pos+1..]
                    // both must be non-empty word chars
                    let repo = &before[slash_pos + 1..];
                    if repo.is_empty() || !repo.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
                        continue;
                    }
                    // Find start of owner (walk back from slash)
                    let owner_end = slash_pos;
                    let owner_start = before[..owner_end]
                        .rfind(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_' || c == '.'))
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    let owner = &before[owner_start..owner_end];
                    if owner.is_empty() || !owner.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
                        continue;
                    }
                    let ref_start = owner_start;
                    let ref_end = digit_end;
                    // Don't overlap with an already found URL
                    let overlaps = splits.iter().any(|&(s, e, _)| ref_start < e && ref_end > s);
                    if !overlaps {
                        splits.push((ref_start, ref_end, true));
                    }
                }
            }

            if splits.is_empty() {
                new_spans.push(span);
                continue;
            }

            // Sort splits by start position
            splits.sort_by_key(|&(s, _, _)| s);

            for &(start, end, _is_link) in &splits {
                if start > last {
                    new_spans.push(Span::styled(text[last..start].to_string(), Style::default()));
                }
                new_spans.push(Span::styled(text[start..end].to_string(), link_style));
                last = end;
            }
            if last < text.len() {
                new_spans.push(Span::styled(text[last..].to_string(), Style::default()));
            }
        }

        line.spans = new_spans;
    }
}

fn highlight_code(code: &str, lang: &str) -> Vec<Line<'static>> {
    let syntax = if lang.is_empty() {
        SYNTAX_SET.find_syntax_plain_text()
    } else {
        SYNTAX_SET
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text())
    };

    let mut h = HighlightLines::new(syntax, &THEME);
    let border_style = Style::new().fg(Color::DarkGray);

    // Top border — left side only (no right border, avoids CJK width issues)
    let mut lines = Vec::new();
    if lang.is_empty() {
        lines.push(Line::from(Span::styled("  ────────────────────", border_style)));
    } else {
        lines.push(Line::from(Span::styled(format!("  ── {} ──────────", lang), border_style)));
    }

    // Code lines — left border only
    let code_trimmed = code.trim_end_matches('\n');
    for line_str in code_trimmed.split('\n') {
        let line_with_nl = format!("{line_str}\n");
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled("  │ ", border_style));

        match h.highlight_line(&line_with_nl, &SYNTAX_SET) {
            Ok(ranges) => {
                for (style, text) in ranges {
                    let fg = style.foreground;
                    let ratatui_style = Style::new().fg(Color::Rgb(fg.r, fg.g, fg.b));
                    let t = text.trim_end_matches('\n');
                    if !t.is_empty() {
                        spans.push(Span::styled(t.to_string(), ratatui_style));
                    }
                }
            }
            Err(_) => {
                spans.push(Span::styled(line_str.to_string(), Style::new().fg(Color::White)));
            }
        }

        lines.push(Line::from(spans));
    }

    lines
}

struct TableBuilder {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    #[allow(dead_code)]
    alignments: Vec<Alignment>,
    current_cell: String,
    in_header: bool,
    current_row: Vec<String>,
}

impl TableBuilder {
    fn new(alignments: Vec<Alignment>) -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            alignments,
            current_cell: String::new(),
            in_header: false,
            current_row: Vec::new(),
        }
    }

    fn render(&self, max_width: usize) -> Vec<Line<'static>> {
        let num_cols = self.headers.len();
        if num_cols == 0 {
            return Vec::new();
        }

        // Calculate column widths using display width (CJK = 2 cols)
        let mut col_widths: Vec<usize> = self
            .headers
            .iter()
            .map(|h| display_width(h))
            .collect();

        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(display_width(cell));
                }
            }
        }

        // Account for borders and padding: │ content │ content │
        // overhead = (num_cols + 1) borders + (num_cols * 2) padding
        let overhead = num_cols + 1 + num_cols * 2;
        let total_content: usize = col_widths.iter().sum();
        let total = total_content + overhead;

        if total > max_width && total_content > 0 {
            let available = max_width.saturating_sub(overhead);
            let mut new_widths: Vec<usize> = col_widths
                .iter()
                .map(|&w| {
                    let proportional = (w as f64 / total_content as f64 * available as f64) as usize;
                    proportional.max(3)
                })
                .collect();
            // Adjust so sum doesn't exceed available
            let sum: usize = new_widths.iter().sum();
            if sum > available {
                let excess = sum - available;
                // Remove excess from the widest column
                if let Some(widest) = new_widths.iter_mut().max() {
                    *widest = widest.saturating_sub(excess);
                    if *widest < 3 {
                        *widest = 3;
                    }
                }
            }
            col_widths = new_widths;
        }

        let border_style = Style::new().fg(Color::DarkGray);
        let mut lines = Vec::new();

        // Top border: ┌──┬──┐
        lines.push(self.horizontal_border(&col_widths, '┌', '┬', '┐', border_style));

        // Header row
        lines.push(self.format_row(&self.headers, &col_widths, border_style, true));

        // Header separator: ├──┼──┤
        lines.push(self.horizontal_border(&col_widths, '├', '┼', '┤', border_style));

        // Data rows
        for row in &self.rows {
            lines.push(self.format_row(row, &col_widths, border_style, false));
        }

        // Bottom border: └──┴──┘
        lines.push(self.horizontal_border(&col_widths, '└', '┴', '┘', border_style));

        lines
    }

    fn horizontal_border(
        &self,
        col_widths: &[usize],
        left: char,
        mid: char,
        right: char,
        style: Style,
    ) -> Line<'static> {
        let mut s = String::new();
        s.push(left);
        for (i, &w) in col_widths.iter().enumerate() {
            // +2 for padding spaces on each side
            for _ in 0..w + 2 {
                s.push('─');
            }
            if i < col_widths.len() - 1 {
                s.push(mid);
            }
        }
        s.push(right);
        Line::from(Span::styled(s, style))
    }

    fn format_row(
        &self,
        cells: &[String],
        col_widths: &[usize],
        border_style: Style,
        is_header: bool,
    ) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.push(Span::styled("│", border_style));

        for (i, width) in col_widths.iter().enumerate() {
            let cell_text = cells.get(i).map(|s| s.as_str()).unwrap_or("");
            let dw = display_width(cell_text);
            let truncated: String = if dw > *width {
                // Truncate by display width
                let mut w = 0;
                cell_text.chars().take_while(|c| {
                    w += if c.is_ascii() { 1 } else { 2 };
                    w <= *width
                }).collect()
            } else {
                cell_text.to_string()
            };
            let padding = width.saturating_sub(display_width(&truncated));

            let content = format!(" {}{} ", truncated, " ".repeat(padding));
            let style = if is_header {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            spans.push(Span::styled(content, style));
            spans.push(Span::styled("│", border_style));
        }

        Line::from(spans)
    }
}

struct MarkdownRenderer {
    style_stack: Vec<Style>,
    current_line: Vec<Span<'static>>,
    lines: Vec<Line<'static>>,
    list_depth: usize,
    list_counters: Vec<Option<u64>>,
    blockquote_depth: usize,
    code_block_lang: Option<String>,
    code_block_content: Option<String>,
    table_builder: Option<TableBuilder>,
}

impl MarkdownRenderer {
    fn new() -> Self {
        Self {
            style_stack: Vec::new(),
            current_line: Vec::new(),
            lines: Vec::new(),
            list_depth: 0,
            list_counters: Vec::new(),
            blockquote_depth: 0,
            code_block_lang: None,
            code_block_content: None,
            table_builder: None,
        }
    }

    fn merged_style(&self) -> Style {
        let mut style = Style::default();
        for s in &self.style_stack {
            if let Some(fg) = s.fg {
                style.fg = Some(fg);
            }
            if let Some(bg) = s.bg {
                style.bg = Some(bg);
            }
            style.add_modifier |= s.add_modifier;
        }
        style
    }

    fn flush_line(&mut self) {
        let mut spans = std::mem::take(&mut self.current_line);
        if self.blockquote_depth > 0 {
            let prefix = "│ ".repeat(self.blockquote_depth);
            spans.insert(0, Span::styled(prefix, Style::new().fg(Color::DarkGray)));
        }
        self.lines.push(Line::from(spans));
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.handle_start(tag),
            Event::End(tag_end) => self.handle_end(tag_end),
            Event::Text(text) => {
                if let Some(ref mut buf) = self.code_block_content {
                    buf.push_str(&text);
                } else if let Some(ref mut tb) = self.table_builder {
                    tb.current_cell.push_str(&text);
                } else {
                    let style = self.merged_style();
                    self.current_line
                        .push(Span::styled(text.to_string(), style));
                }
            }
            Event::Code(text) => {
                self.current_line.push(Span::styled(
                    text.to_string(),
                    Style::new().fg(Color::Yellow),
                ));
            }
            Event::SoftBreak => {
                let style = self.merged_style();
                self.current_line.push(Span::styled(" ", style));
            }
            Event::HardBreak => {
                self.flush_line();
            }
            Event::Rule => {
                self.flush_line();
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(40),
                    Style::new().fg(Color::DarkGray),
                )));
                self.flush_line();
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                let style = self.merged_style();
                self.current_line
                    .push(Span::styled(html.to_string(), style));
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "☑ " } else { "☐ " };
                self.current_line
                    .push(Span::styled(marker, Style::default()));
            }
            _ => {}
        }
    }

    fn handle_start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {}
            Tag::Heading { level, .. } => {
                let style = match level {
                    HeadingLevel::H1 => Style::new()
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED),
                    _ => Style::new().add_modifier(Modifier::BOLD),
                };
                self.style_stack.push(style);
            }
            Tag::Strong => {
                self.style_stack
                    .push(Style::new().add_modifier(Modifier::BOLD));
            }
            Tag::Emphasis => {
                self.style_stack
                    .push(Style::new().add_modifier(Modifier::ITALIC));
            }
            Tag::Strikethrough => {
                self.style_stack
                    .push(Style::new().add_modifier(Modifier::CROSSED_OUT));
            }
            Tag::BlockQuote(_) => {
                self.blockquote_depth += 1;
                self.style_stack
                    .push(Style::new().add_modifier(Modifier::ITALIC));
            }
            Tag::List(start) => {
                self.list_depth += 1;
                self.list_counters.push(start);
            }
            Tag::Item => {
                self.flush_line();
                let indent = "  ".repeat(self.list_depth);
                let prefix = if let Some(Some(n)) = self.list_counters.last_mut() {
                    let p = format!("{indent}{n}. ");
                    *n += 1;
                    p
                } else {
                    format!("{indent}- ")
                };
                self.current_line
                    .push(Span::styled(prefix, Style::default()));
            }
            Tag::Link { .. } => {
                self.style_stack.push(
                    Style::new()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::UNDERLINED),
                );
            }
            Tag::CodeBlock(kind) => {
                if !self.current_line.is_empty() {
                    self.flush_line();
                }
                let lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        let l = lang.to_string();
                        if l.is_empty() { None } else { Some(l) }
                    }
                    CodeBlockKind::Indented => None,
                };
                self.code_block_lang = lang;
                self.code_block_content = Some(String::new());
            }
            Tag::Table(alignments) => {
                self.flush_line();
                self.table_builder = Some(TableBuilder::new(alignments));
            }
            Tag::TableHead => {
                if let Some(ref mut tb) = self.table_builder {
                    tb.in_header = true;
                    tb.current_row.clear();
                }
            }
            Tag::TableRow => {
                if let Some(ref mut tb) = self.table_builder {
                    tb.current_row.clear();
                }
            }
            Tag::TableCell => {
                if let Some(ref mut tb) = self.table_builder {
                    tb.current_cell.clear();
                }
            }
            _ => {}
        }
    }

    fn handle_end(&mut self, tag_end: TagEnd) {
        match tag_end {
            TagEnd::Paragraph => {
                self.flush_line();
                // Blank line after paragraph
                self.flush_line();
            }
            TagEnd::Heading(_) => {
                self.style_stack.pop();
                self.flush_line();
                // Blank line after heading
                self.flush_line();
            }
            TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link => {
                self.style_stack.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.style_stack.pop();
                self.blockquote_depth -= 1;
                self.flush_line();
            }
            TagEnd::List(_) => {
                self.list_depth -= 1;
                self.list_counters.pop();
                if self.list_depth == 0 {
                    self.flush_line();
                }
            }
            TagEnd::Item => {
                self.flush_line();
            }
            TagEnd::CodeBlock => {
                let lang = self.code_block_lang.take().unwrap_or_default();
                let code = self.code_block_content.take().unwrap_or_default();
                let highlighted = highlight_code(&code, &lang);
                self.lines.extend(highlighted);
            }
            TagEnd::TableCell => {
                if let Some(ref mut tb) = self.table_builder {
                    let cell = std::mem::take(&mut tb.current_cell);
                    tb.current_row.push(cell.trim().to_string());
                }
            }
            TagEnd::TableHead => {
                if let Some(ref mut tb) = self.table_builder {
                    tb.headers = std::mem::take(&mut tb.current_row);
                    tb.in_header = false;
                }
            }
            TagEnd::TableRow => {
                if let Some(ref mut tb) = self.table_builder {
                    if !tb.in_header {
                        let row = std::mem::take(&mut tb.current_row);
                        tb.rows.push(row);
                    }
                }
            }
            TagEnd::Table => {
                if let Some(tb) = self.table_builder.take() {
                    let rendered = tb.render(80);
                    self.lines.extend(rendered);
                }
            }
            _ => {}
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        if !self.current_line.is_empty() {
            self.flush_line();
        }
        // Trim trailing blank lines
        while self
            .lines
            .last()
            .is_some_and(|l| l.spans.is_empty() || l.spans.iter().all(|s| s.content.is_empty()))
        {
            self.lines.pop();
        }
        self.lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let lines = parse_markdown("hello world");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[0].spans[0].content, "hello world");
    }

    #[test]
    fn test_bold() {
        let lines = parse_markdown("**bold text**");
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert_eq!(span.content, "bold text");
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_italic() {
        let lines = parse_markdown("*italic text*");
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert_eq!(span.content, "italic text");
        assert!(span.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_inline_code() {
        let lines = parse_markdown("use `foo()` here");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 3);
        assert_eq!(lines[0].spans[1].content, "foo()");
        assert_eq!(lines[0].spans[1].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_heading_h1() {
        let lines = parse_markdown("# Title");
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert_eq!(span.content, "Title");
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
        assert!(span.style.add_modifier.contains(Modifier::ITALIC));
        assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn test_heading_h2() {
        let lines = parse_markdown("## Subtitle");
        let span = &lines[0].spans[0];
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
        assert!(!span.style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn test_nested_bold_italic() {
        let lines = parse_markdown("**bold _and italic_**");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_multiline_paragraph() {
        let lines = parse_markdown("line one\nline two");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_multiple_paragraphs() {
        let lines = parse_markdown("para one\n\npara two");
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_thematic_break() {
        let lines = parse_markdown("above\n\n---\n\nbelow");
        let hr_line = lines
            .iter()
            .find(|l| l.spans.iter().any(|s| s.content.contains('─')));
        assert!(hr_line.is_some());
    }

    #[test]
    fn test_code_block_plain() {
        let md = "```\nfn main() {}\n```";
        let lines = parse_markdown(md);
        assert!(lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains("main"))));
    }

    #[test]
    fn test_code_block_with_language() {
        let md = "```rust\nlet x = 42;\n```";
        let lines = parse_markdown(md);
        let code_spans: Vec<&Span> = lines.iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.content.contains("42") || s.content.contains("let"))
            .collect();
        assert!(!code_spans.is_empty());
    }

    #[test]
    fn test_code_block_border() {
        let md = "```rust\ncode\n```";
        let lines = parse_markdown(md);
        // Top border uses ── style (no box corners)
        assert!(lines[0].spans.iter().any(|s| s.content.contains('─')));
        // Code line has left border │
        assert!(lines[1].spans.iter().any(|s| s.content.contains('│')));
    }

    #[test]
    fn test_code_block_language_label() {
        let md = "```python\nprint('hi')\n```";
        let lines = parse_markdown(md);
        assert!(lines[0].spans.iter().any(|s| s.content.contains("python")));
    }

    #[test]
    fn test_code_block_unknown_language() {
        let md = "```obscurelang\nsome code\n```";
        let lines = parse_markdown(md);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_simple_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let lines = parse_markdown(md);
        assert!(lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains('┌'))));
        assert!(lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains('├'))));
        assert!(lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains('└'))));
    }

    #[test]
    fn test_table_content() {
        let md = "| Name | Val |\n|---|---|\n| foo | bar |";
        let lines = parse_markdown(md);
        let all_text: String = lines.iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(all_text.contains("foo"));
        assert!(all_text.contains("bar"));
    }

    #[test]
    fn test_table_header_bold() {
        let md = "| H1 | H2 |\n|---|---|\n| a | b |";
        let lines = parse_markdown(md);
        let header_spans: Vec<&Span> = lines.iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.content.trim() == "H1" || s.content.trim() == "H2")
            .collect();
        assert!(header_spans.iter().any(|s| s.style.add_modifier.contains(Modifier::BOLD)));
    }

    #[test]
    fn test_auto_link_url() {
        let lines = parse_markdown("visit https://example.com for info");
        let link_style = Style::new().fg(Color::Blue).add_modifier(Modifier::UNDERLINED);
        let url_span = lines.iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("https://example.com"));
        assert!(url_span.is_some(), "URL span should exist");
        assert_eq!(url_span.unwrap().style, link_style);
    }

    #[test]
    fn test_auto_link_github_ref() {
        let lines = parse_markdown("see owner/repo#123 for details");
        let link_style = Style::new().fg(Color::Blue).add_modifier(Modifier::UNDERLINED);
        let ref_span = lines.iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("owner/repo#123"));
        assert!(ref_span.is_some(), "GitHub ref span should exist");
        assert_eq!(ref_span.unwrap().style, link_style);
    }

    #[test]
    fn test_auto_link_preserves_styled_spans() {
        // Bold text should not be re-styled even if it contains a URL pattern
        let lines = parse_markdown("**https://bold.com**");
        let url_span = lines.iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("https://bold.com"));
        assert!(url_span.is_some());
        // Should keep bold style, not link style
        assert!(url_span.unwrap().style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_blockquote_prefix() {
        let lines = parse_markdown("> quoted text");
        assert!(lines[0].spans.iter().any(|s| s.content.contains('│')));
    }

    #[test]
    fn test_auto_link_no_false_positive() {
        let lines = parse_markdown("just plain text here");
        // All spans should have default style (no link styling)
        let link_style = Style::new().fg(Color::Blue).add_modifier(Modifier::UNDERLINED);
        let has_link = lines.iter()
            .flat_map(|l| l.spans.iter())
            .any(|s| s.style == link_style);
        assert!(!has_link);
    }
}
