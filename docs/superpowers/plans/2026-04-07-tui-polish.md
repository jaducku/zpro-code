# TUI Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring atom-code-rs TUI to feature parity with original TS Claude Code — markdown rendering, syntax highlighting, tables, hyperlinks, input history, multi-line input, enhanced status bar, tool result display.

**Architecture:** New `src/tui/markdown.rs` module converts pulldown-cmark events to ratatui `Vec<Line>`. New `src/tui/history.rs` manages persistent input history. Existing `ui.rs` and `app.rs` are modified for multi-line input, status bar, and tool result changes.

**Tech Stack:** pulldown-cmark 0.12, syntect 5, ratatui 0.29, crossterm, serde_json

**Spec:** `docs/superpowers/specs/2026-04-07-tui-polish-design.md`

---

## File Map

| File | Action | Responsibility |
|------|--------|----------------|
| `src/tui/markdown.rs` | Create | Markdown→ratatui conversion, code highlighting, tables, links |
| `src/tui/history.rs` | Create | Input history load/save/navigate |
| `src/tui/ui.rs` | Modify | Use markdown renderer, dynamic input height, enhanced status bar, tool result full display |
| `src/tui/app.rs` | Modify | Multi-line input handling, history integration, new App fields (context_window, permission_mode) |
| `src/tui/run.rs` | Modify | Kitty protocol enable, key binding changes (Alt+Enter, Up/Down for history) |
| `src/tui/mod.rs` | Modify | Add `pub mod markdown; pub mod history;` |

---

## Task 1: Markdown Renderer — Basic Text Formatting

**Files:**
- Create: `src/tui/markdown.rs`
- Modify: `src/tui/mod.rs` (add module declaration)
- Test: inline `#[cfg(test)]` in `markdown.rs`

- [ ] **Step 1: Write failing tests for basic markdown parsing**

```rust
// src/tui/markdown.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let lines = parse_markdown("hello world");
        assert_eq!(lines.len(), 1);
        // Should have one span with no special styling
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
        assert_eq!(lines[0].spans.len(), 3); // "use ", "foo()", " here"
        assert_eq!(lines[0].spans[1].content, "foo()");
        assert_eq!(lines[0].spans[1].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn test_heading_h1() {
        let lines = parse_markdown("# Title");
        // finish() trims trailing blank lines, so just heading line remains
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
        // Should produce spans with combined styles
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_multiline_paragraph() {
        let lines = parse_markdown("line one\nline two");
        // pulldown-cmark treats single newlines as soft breaks within a paragraph
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_multiple_paragraphs() {
        let lines = parse_markdown("para one\n\npara two");
        // Two paragraphs should have a blank line between them
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_thematic_break() {
        let lines = parse_markdown("above\n\n---\n\nbelow");
        // Should contain a line of '─' characters
        let hr_line = lines.iter().find(|l| {
            l.spans.iter().any(|s| s.content.contains('─'))
        });
        assert!(hr_line.is_some());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tui::markdown::tests -- --nocapture 2>&1 | head -30`
Expected: Compilation error — `parse_markdown` not defined

- [ ] **Step 3: Implement MarkdownRenderer and parse_markdown**

```rust
// src/tui/markdown.rs
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, CodeBlockKind, HeadingLevel};
use ratatui::prelude::*;
use ratatui::style::{Color, Modifier, Style};

/// Parse markdown text into styled ratatui Lines.
pub fn parse_markdown(text: &str) -> Vec<Line<'static>> {
    let mut renderer = MarkdownRenderer::new();
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let parser = Parser::new_ext(text, opts);

    for event in parser {
        renderer.handle_event(event);
    }

    renderer.finish()
}

struct MarkdownRenderer {
    style_stack: Vec<Style>,
    current_line: Vec<Span<'static>>,
    lines: Vec<Line<'static>>,
    list_depth: usize,
    list_counters: Vec<Option<u64>>, // None=unordered, Some(n)=ordered
    blockquote_depth: usize,
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
        }
    }

    fn merged_style(&self) -> Style {
        let mut style = Style::default();
        for s in &self.style_stack {
            if let Some(fg) = s.fg {
                style = style.fg(fg);
            }
            if let Some(bg) = s.bg {
                style = style.bg(bg);
            }
            style.add_modifier |= s.add_modifier;
        }
        style
    }

    fn flush_line(&mut self) {
        let spans = std::mem::take(&mut self.current_line);
        if !spans.is_empty() {
            self.lines.push(Line::from(spans));
        }
    }

    fn push_blank_line(&mut self) {
        self.flush_line();
        self.lines.push(Line::from(""));
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.handle_start(tag),
            Event::End(tag) => self.handle_end(tag),
            Event::Text(text) => {
                let style = self.merged_style();
                self.current_line.push(Span::styled(text.to_string(), style));
            }
            Event::Code(text) => {
                // Inline code — leaf event, not Start/End pair
                self.current_line.push(Span::styled(
                    text.to_string(),
                    Style::new().fg(Color::Yellow),
                ));
            }
            Event::SoftBreak => {
                // Soft break within paragraph — treat as space
                let style = self.merged_style();
                self.current_line.push(Span::styled(" ", style));
            }
            Event::HardBreak => {
                self.flush_line();
            }
            Event::Rule => {
                self.flush_line();
                // Horizontal rule: 40 '─' characters
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(40),
                    Style::new().fg(Color::DarkGray),
                )));
                self.push_blank_line();
            }
            // Table, CodeBlock, etc. handled in later tasks
            _ => {}
        }
    }

    fn handle_start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => {
                // No action needed at start — text events handle content
            }
            Tag::Heading { level, .. } => {
                self.flush_line();
                let style = match level {
                    HeadingLevel::H1 => Style::new()
                        .add_modifier(Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED),
                    HeadingLevel::H2 => Style::new().bold(),
                    _ => Style::new().bold(),
                };
                self.style_stack.push(style);
            }
            Tag::Strong => {
                self.style_stack.push(Style::new().bold());
            }
            Tag::Emphasis => {
                self.style_stack.push(Style::new().italic());
            }
            Tag::BlockQuote(_) => {
                self.flush_line();
                self.blockquote_depth += 1;
                self.style_stack.push(Style::new().italic());
            }
            Tag::List(start) => {
                self.flush_line();
                self.list_depth += 1;
                self.list_counters.push(start);
            }
            Tag::Item => {
                self.flush_line();
                let indent = "  ".repeat(self.list_depth);
                let prefix = match self.list_counters.last_mut() {
                    Some(Some(n)) => {
                        let p = format!("{}{}. ", indent, n);
                        *n += 1;
                        p
                    }
                    _ => format!("{}- ", indent),
                };
                self.current_line.push(Span::styled(prefix, Style::default()));
            }
            Tag::Link { dest_url, .. } => {
                self.style_stack.push(Style::new().fg(Color::Blue).underlined());
                // Store URL — could be used for OSC 8 in future
                let _ = dest_url;
            }
            Tag::CodeBlock(_kind) => {
                // Handled in Task 2 — for now collect as plain text
                self.flush_line();
                self.style_stack.push(Style::new().fg(Color::DarkGray));
            }
            Tag::Table(_) => {
                // Handled in Task 3
                self.flush_line();
            }
            _ => {}
        }
    }

    fn handle_end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush_line();
                self.lines.push(Line::from("")); // blank line after paragraph
            }
            TagEnd::Heading(_) => {
                self.style_stack.pop();
                self.flush_line();
                self.lines.push(Line::from("")); // blank line after heading
            }
            TagEnd::Strong | TagEnd::Emphasis => {
                self.style_stack.pop();
            }
            TagEnd::BlockQuote(_) => {
                self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                self.style_stack.pop();
                self.flush_line();
            }
            TagEnd::List(_) => {
                self.list_depth = self.list_depth.saturating_sub(1);
                self.list_counters.pop();
                self.flush_line();
            }
            TagEnd::Item => {
                self.flush_line();
            }
            TagEnd::Link => {
                self.style_stack.pop();
            }
            TagEnd::CodeBlock => {
                self.style_stack.pop();
                self.flush_line();
            }
            _ => {}
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush_line();
        // Trim trailing blank lines
        while self.lines.last().map_or(false, |l| l.spans.is_empty()) {
            self.lines.pop();
        }
        self.lines
    }
}
```

- [ ] **Step 4: Add module declaration**

In `src/tui/mod.rs`, add:
```rust
pub mod markdown;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib tui::markdown::tests -- --nocapture`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add src/tui/markdown.rs src/tui/mod.rs
git commit -m "feat(tui): add markdown renderer with basic text formatting"
```

---

## Task 2: Code Block Syntax Highlighting

**Files:**
- Modify: `src/tui/markdown.rs`
- Test: inline `#[cfg(test)]` in `markdown.rs`

- [ ] **Step 1: Write failing tests for code blocks**

```rust
#[test]
fn test_code_block_plain() {
    let md = "```\nfn main() {}\n```";
    let lines = parse_markdown(md);
    // Should have bordered code block
    assert!(lines.iter().any(|l| l.spans.iter().any(|s| s.content.contains("main"))));
}

#[test]
fn test_code_block_with_language() {
    let md = "```rust\nlet x = 42;\n```";
    let lines = parse_markdown(md);
    // Should have syntax-highlighted spans (multiple colors)
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
    // First line should contain top border '┌'
    assert!(lines[0].spans.iter().any(|s| s.content.contains('┌')));
    // Last code line should have bottom border '└'
    assert!(lines.last().unwrap().spans.iter().any(|s| s.content.contains('└')));
}

#[test]
fn test_code_block_language_label() {
    let md = "```python\nprint('hi')\n```";
    let lines = parse_markdown(md);
    // Top border should contain language name
    assert!(lines[0].spans.iter().any(|s| s.content.contains("python")));
}

#[test]
fn test_code_block_unknown_language() {
    let md = "```obscurelang\nsome code\n```";
    let lines = parse_markdown(md);
    // Should still render (plain text, no highlighting) without panic
    assert!(!lines.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tui::markdown::tests::test_code_block -- --nocapture`
Expected: FAIL — code blocks currently render as plain gray text without borders

- [ ] **Step 3: Implement code block handling with syntect**

Add to `markdown.rs`:

```rust
use std::sync::LazyLock;
use syntect::highlighting::{ThemeSet, Theme};
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME: LazyLock<Theme> = LazyLock::new(|| {
    let ts = ThemeSet::load_defaults();
    ts.themes["base16-ocean.dark"].clone()
});

fn highlight_code(code: &str, lang: &str) -> Vec<Line<'static>> {
    let syntax = SYNTAX_SET.find_syntax_by_token(lang)
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let mut h = HighlightLines::new(syntax, &THEME);
    let mut code_lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut max_width: usize = 0;

    for line in code.lines() {
        let spans: Vec<Span<'static>> = match h.highlight_line(line, &SYNTAX_SET) {
            Ok(ranges) => {
                ranges.into_iter().map(|(style, text)| {
                    Span::styled(
                        text.to_string(),
                        Style::new().fg(Color::Rgb(
                            style.foreground.r,
                            style.foreground.g,
                            style.foreground.b,
                        )),
                    )
                }).collect()
            }
            Err(_) => vec![Span::raw(line.to_string())], // fallback
        };
        let line_width: usize = line.len();
        max_width = max_width.max(line_width);
        code_lines.push(spans);
    }

    // Build bordered output
    let box_width = max_width + 4; // "│ " + content + " │"
    let border_style = Style::new().fg(Color::DarkGray);

    let mut result = Vec::new();

    // Top border: ┌─ lang ────┐
    let lang_label = if lang.is_empty() || lang == "Plain Text" { String::new() } else { format!(" {} ", lang) };
    let top_dashes = "─".repeat(box_width.saturating_sub(2 + lang_label.len()));
    result.push(Line::from(vec![
        Span::styled("  ┌─", border_style),
        Span::styled(lang_label, border_style),
        Span::styled(format!("{}┐", top_dashes), border_style),
    ]));

    // Code lines: │ content │
    for spans in code_lines {
        let mut line_spans = vec![Span::styled("  │ ", border_style)];
        line_spans.extend(spans);
        // Right padding + border would add complexity; just use left border
        result.push(Line::from(line_spans));
    }

    // Bottom border: └────────┘
    let bottom = "─".repeat(box_width.saturating_sub(2));
    result.push(Line::from(Span::styled(
        format!("  └{}┘", bottom),
        border_style,
    )));

    result
}
```

Update `handle_start` and `handle_end` for `CodeBlock`:

```rust
// In handle_start:
Tag::CodeBlock(kind) => {
    self.flush_line();
    let lang = match &kind {
        CodeBlockKind::Fenced(lang) => lang.to_string(),
        CodeBlockKind::Indented => String::new(),
    };
    // Push a marker — actual content collected via Text events,
    // then highlighted in handle_end
    self.style_stack.push(Style::default()); // placeholder
    self.code_block_lang = Some(lang);
    self.code_block_content = Some(String::new());
}

// In handle_event Text, check if in code block:
Event::Text(text) => {
    if let Some(ref mut content) = self.code_block_content {
        content.push_str(&text);
    } else {
        let style = self.merged_style();
        self.current_line.push(Span::styled(text.to_string(), style));
    }
}

// In handle_end:
TagEnd::CodeBlock => {
    self.style_stack.pop();
    if let (Some(lang), Some(content)) = (self.code_block_lang.take(), self.code_block_content.take()) {
        let highlighted = highlight_code(&content, &lang);
        self.lines.extend(highlighted);
    }
}
```

Add fields to `MarkdownRenderer`:
```rust
code_block_lang: Option<String>,
code_block_content: Option<String>,
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tui::markdown::tests -- --nocapture`
Expected: All tests pass (basic + code block)

- [ ] **Step 5: Commit**

```bash
git add src/tui/markdown.rs
git commit -m "feat(tui): add code block syntax highlighting with syntect"
```

---

## Task 3: Table Rendering

**Files:**
- Modify: `src/tui/markdown.rs`

- [ ] **Step 1: Write failing tests for tables**

```rust
#[test]
fn test_simple_table() {
    let md = "| A | B |\n|---|---|\n| 1 | 2 |";
    let lines = parse_markdown(md);
    // Should have box-drawing borders
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
    // Header cells should be bold
    let header_spans: Vec<&Span> = lines.iter()
        .flat_map(|l| l.spans.iter())
        .filter(|s| s.content.trim() == "H1" || s.content.trim() == "H2")
        .collect();
    assert!(header_spans.iter().any(|s| s.style.add_modifier.contains(Modifier::BOLD)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tui::markdown::tests::test_simple_table -- --nocapture`
Expected: FAIL

- [ ] **Step 3: Implement table rendering**

Add `TableBuilder` and `render_table()` to `markdown.rs`:

```rust
struct TableBuilder {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    alignments: Vec<pulldown_cmark::Alignment>,
    current_cell: String,
    in_header: bool,
    current_row: Vec<String>,
}

impl TableBuilder {
    fn new(alignments: Vec<pulldown_cmark::Alignment>) -> Self {
        Self {
            headers: Vec::new(),
            rows: Vec::new(),
            alignments,
            current_cell: String::new(),
            in_header: false,
            current_row: Vec::new(),
        }
    }

    fn render(self, max_width: usize) -> Vec<Line<'static>> {
        let ncols = self.headers.len();
        if ncols == 0 { return Vec::new(); }

        // Calculate column widths
        let mut widths: Vec<usize> = self.headers.iter().map(|h| h.len()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        // Width overflow: proportional shrink, min 3 chars per column
        let border_overhead = ncols * 3 + 5; // "  │" + " content " per col + "│"
        let content_budget = max_width.saturating_sub(border_overhead);
        let total_content: usize = widths.iter().sum();
        if total_content > content_budget && content_budget > 0 {
            let scale = content_budget as f64 / total_content as f64;
            for w in &mut widths {
                *w = (*w as f64 * scale).ceil().max(3.0) as usize;
            }
        }

        let border = Style::new().fg(Color::DarkGray);
        let mut lines = Vec::new();

        // Top border
        let top: String = widths.iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┬");
        lines.push(Line::from(Span::styled(format!("  ┌{}┐", top), border)));

        // Header row
        let header_spans = self.format_row(&self.headers, &widths, true);
        lines.push(header_spans);

        // Header separator
        let sep: String = widths.iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┼");
        lines.push(Line::from(Span::styled(format!("  ├{}┤", sep), border)));

        // Data rows
        for row in &self.rows {
            lines.push(self.format_row(row, &widths, false));
        }

        // Bottom border
        let bottom: String = widths.iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┴");
        lines.push(Line::from(Span::styled(format!("  └{}┘", bottom), border)));

        lines
    }

    fn format_row(&self, cells: &[String], widths: &[usize], is_header: bool) -> Line<'static> {
        let border = Style::new().fg(Color::DarkGray);
        let cell_style = if is_header {
            Style::new().bold()
        } else {
            Style::default()
        };

        let mut spans = vec![Span::styled("  │", border)];
        for (i, width) in widths.iter().enumerate() {
            let content = cells.get(i).map(|s| s.as_str()).unwrap_or("");
            let padded = format!(" {:width$} ", content, width = width);
            spans.push(Span::styled(padded, cell_style));
            spans.push(Span::styled("│", border));
        }
        Line::from(spans)
    }
}
```

Update `handle_start`/`handle_end` for Table events:

```rust
// Add field to MarkdownRenderer:
table_builder: Option<TableBuilder>,

// In handle_start:
Tag::Table(alignments) => {
    self.flush_line();
    self.table_builder = Some(TableBuilder::new(alignments));
}
Tag::TableHead => {
    if let Some(ref mut tb) = self.table_builder {
        tb.in_header = true;
    }
}
Tag::TableRow => {
    if let Some(ref mut tb) = self.table_builder {
        tb.current_row = Vec::new();
    }
}
Tag::TableCell => {
    if let Some(ref mut tb) = self.table_builder {
        tb.current_cell = String::new();
    }
}

// In handle_event Text, check table context:
Event::Text(text) => {
    if let Some(ref mut content) = self.code_block_content {
        content.push_str(&text);
    } else if let Some(ref mut tb) = self.table_builder {
        tb.current_cell.push_str(&text);
    } else {
        let style = self.merged_style();
        self.current_line.push(Span::styled(text.to_string(), style));
    }
}

// In handle_end:
TagEnd::TableCell => {
    if let Some(ref mut tb) = self.table_builder {
        let cell = std::mem::take(&mut tb.current_cell);
        tb.current_row.push(cell);
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
        // max_width stored as field on MarkdownRenderer (set via parse_markdown_with_width)
        let table_lines = tb.render(self.max_width);
        self.lines.extend(table_lines);
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib tui::markdown::tests -- --nocapture`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/tui/markdown.rs
git commit -m "feat(tui): add markdown table rendering with box-drawing borders"
```

---

## Task 4: Integrate Markdown Renderer into UI

**Files:**
- Modify: `src/tui/ui.rs` (lines 189-196 — assistant message rendering)

- [ ] **Step 1: Replace plain text rendering with markdown**

In `ui.rs`, change the `DisplayMessage::Text { role: TextRole::Assistant, content }` arm:

```rust
// Replace lines 189-196:
DisplayMessage::Text { role: TextRole::Assistant, content } => {
    lines.push(Line::from(""));
    let md_lines = crate::tui::markdown::parse_markdown(content);
    for line in md_lines {
        let mut prefixed: Vec<Span<'static>> = vec![Span::raw("  ")];
        prefixed.extend(line.spans);
        lines.push(Line::from(prefixed));
    }
}
```

- [ ] **Step 2: Add URL/GitHub auto-detection in markdown.rs**

In `markdown.rs`, add a post-processing function that scans plain text spans for URLs and GitHub refs:

```rust
/// Post-process lines to auto-detect and style URLs and GitHub issue refs in plain text.
fn auto_detect_links(lines: &mut [Line<'static>]) {
    let url_re = regex::Regex::new(r"https?://\S+").unwrap();
    let gh_re = regex::Regex::new(r"[a-zA-Z0-9_-]+/[a-zA-Z0-9_.-]+#\d+").unwrap();
    let link_style = Style::new().fg(Color::Blue).underlined();

    for line in lines.iter_mut() {
        let mut new_spans: Vec<Span<'static>> = Vec::new();
        for span in std::mem::take(&mut line.spans) {
            // Only process unstyled (plain) spans
            if span.style != Style::default() {
                new_spans.push(span);
                continue;
            }
            let text = span.content.to_string();
            let mut last_end = 0;
            // Collect all matches (URL and GitHub refs)
            let mut matches: Vec<(usize, usize)> = Vec::new();
            for m in url_re.find_iter(&text) {
                matches.push((m.start(), m.end()));
            }
            for m in gh_re.find_iter(&text) {
                matches.push((m.start(), m.end()));
            }
            matches.sort_by_key(|m| m.0);
            matches.dedup();

            for (start, end) in matches {
                if start > last_end {
                    new_spans.push(Span::raw(text[last_end..start].to_string()));
                }
                new_spans.push(Span::styled(text[start..end].to_string(), link_style));
                last_end = end;
            }
            if last_end < text.len() {
                new_spans.push(Span::raw(text[last_end..].to_string()));
            }
        }
        line.spans = new_spans;
    }
}
```

Call `auto_detect_links(&mut lines)` at the end of `parse_markdown` before returning.

Note: Add `regex` to Cargo.toml if not present. Check with `grep regex Cargo.toml`.

- [ ] **Step 3: Run full test suite to verify no regressions**

Run: `cargo test --lib`
Expected: All 595+ tests pass

- [ ] **Step 4: Commit**

```bash
git add src/tui/ui.rs src/tui/markdown.rs
git commit -m "feat(tui): integrate markdown renderer for assistant messages with auto-link detection"
```

---

## Task 5: Tool Result Display Improvements

**Files:**
- Modify: `src/tui/ui.rs` (tool call rendering, lines 233-250)
- Modify: `src/tui/app.rs` (default collapsed value, lines 580, 646)

- [ ] **Step 1: Change default collapsed to false**

In `app.rs`, find `ToolCallStart` handler (~line 580) and `AgentCompleted` handler (~line 646):

```rust
// ToolCallStart — change collapsed: true → false
collapsed: false,

// AgentCompleted — change collapsed: true → false
collapsed: false,
```

- [ ] **Step 2: Remove 5-line preview limit in ui.rs**

Replace the tool result rendering in `render_conversation` (lines 234-252 in ui.rs):

```rust
// Replace for ToolCall:
if !collapsed {
    if let Some(ref result_text) = result {
        for line_text in result_text.lines() {
            lines.push(Line::from(vec![
                Span::styled("    │ ", Style::default().fg(Color::DarkGray)),
                Span::styled(line_text.to_string(), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }
}
```

Apply same change for Agent result rendering (lines 282-299 in ui.rs).

- [ ] **Step 3: Run tests**

Run: `cargo test --lib`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add src/tui/ui.rs src/tui/app.rs
git commit -m "feat(tui): expand tool results by default, remove 5-line preview limit"
```

---

## Task 6: Input History

**Files:**
- Create: `src/tui/history.rs`
- Modify: `src/tui/mod.rs` (add module declaration)
- Modify: `src/tui/app.rs` (add history field, wire Up/Down)
- Modify: `src/tui/run.rs` (load history on startup)

- [ ] **Step 1: Write failing tests for InputHistory**

```rust
// src/tui/history.rs
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_push_and_up() {
        let mut h = InputHistory::new_in_memory(500);
        h.push("first".into());
        h.push("second".into());
        assert_eq!(h.up("current"), Some("second"));
        assert_eq!(h.up("current"), Some("first"));
        assert_eq!(h.up("current"), None); // at beginning
    }

    #[test]
    fn test_down_restores_stashed() {
        let mut h = InputHistory::new_in_memory(500);
        h.push("old".into());
        let _ = h.up("typing");
        assert_eq!(h.down(), Some("typing")); // restores stashed
        assert_eq!(h.down(), None); // already at end
    }

    #[test]
    fn test_reset() {
        let mut h = InputHistory::new_in_memory(500);
        h.push("entry".into());
        let _ = h.up("x");
        h.reset();
        assert_eq!(h.cursor, None);
        assert_eq!(h.stashed, None);
    }

    #[test]
    fn test_max_entries() {
        let mut h = InputHistory::new_in_memory(3);
        h.push("a".into());
        h.push("b".into());
        h.push("c".into());
        h.push("d".into());
        assert_eq!(h.entries.len(), 3);
        assert_eq!(h.entries[0], "b"); // oldest dropped
    }

    #[test]
    fn test_no_duplicate_consecutive() {
        let mut h = InputHistory::new_in_memory(500);
        h.push("same".into());
        h.push("same".into());
        assert_eq!(h.entries.len(), 1);
    }

    #[test]
    fn test_file_persistence() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_path_buf();

        let mut h = InputHistory::new(path.clone(), 500);
        h.push("hello".into());
        h.push("world".into());

        let h2 = InputHistory::new(path, 500);
        assert_eq!(h2.entries, vec!["hello", "world"]);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tui::history::tests -- --nocapture`
Expected: Compilation error

- [ ] **Step 3: Implement InputHistory**

```rust
// src/tui/history.rs
use std::path::PathBuf;

pub struct InputHistory {
    pub entries: Vec<String>,
    pub cursor: Option<usize>,
    pub stashed: Option<String>,
    max_entries: usize,
    file_path: Option<PathBuf>,
}

impl InputHistory {
    pub fn new(file_path: PathBuf, max_entries: usize) -> Self {
        let entries = Self::load_from_file(&file_path).unwrap_or_default();
        Self {
            entries,
            cursor: None,
            stashed: None,
            max_entries,
            file_path: Some(file_path),
        }
    }

    pub fn new_in_memory(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: None,
            stashed: None,
            max_entries,
            file_path: None,
        }
    }

    pub fn push(&mut self, entry: String) {
        if entry.trim().is_empty() {
            return;
        }
        // Skip consecutive duplicates
        if self.entries.last().map_or(false, |last| last == &entry) {
            return;
        }
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        self.save_to_file();
    }

    /// Move up through history. Returns the entry to display, or None if at the beginning.
    pub fn up(&mut self, current_input: &str) -> Option<&str> {
        match self.cursor {
            None => {
                if self.entries.is_empty() {
                    return None;
                }
                self.stashed = Some(current_input.to_string());
                self.cursor = Some(self.entries.len() - 1);
                Some(&self.entries[self.entries.len() - 1])
            }
            Some(0) => None, // already at oldest
            Some(ref mut idx) => {
                *idx -= 1;
                Some(&self.entries[*idx])
            }
        }
    }

    /// Move down through history. Returns the entry or stashed input, or None if already at end.
    pub fn down(&mut self) -> Option<&str> {
        match self.cursor {
            None => None,
            Some(idx) => {
                if idx + 1 < self.entries.len() {
                    self.cursor = Some(idx + 1);
                    Some(&self.entries[idx + 1])
                } else {
                    // Return to stashed input
                    self.cursor = None;
                    self.stashed.as_deref()
                }
            }
        }
    }

    pub fn reset(&mut self) {
        self.cursor = None;
        self.stashed = None;
    }

    fn load_from_file(path: &PathBuf) -> Option<Vec<String>> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn save_to_file(&self) {
        if let Some(ref path) = self.file_path {
            if let Ok(json) = serde_json::to_string(&self.entries) {
                let _ = std::fs::write(path, json);
            }
        }
    }
}
```

- [ ] **Step 4: Add module declaration**

In `src/tui/mod.rs`, add:
```rust
pub mod history;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib tui::history::tests -- --nocapture`
Expected: All tests pass. Note: `tempfile` may need to be added as a dev-dependency if not present. Check with `grep tempfile Cargo.toml`. If missing, add `tempfile = "3"` under `[dev-dependencies]`.

- [ ] **Step 6: Wire history into App**

In `app.rs`:

Add field to `App`:
```rust
pub history: crate::tui::history::InputHistory,
```

Initialize in `App::new()`:
```rust
history: crate::tui::history::InputHistory::new(
    crate::utils::paths::global_config_dir().join("history.json"),
    500,
),
```

Change `KeyCode::Up` and `KeyCode::Down` in `handle_key()` (lines 315-320):
```rust
KeyCode::Up => {
    if let Some(prev) = self.history.up(&self.input) {
        self.input = prev.to_string();
        self.cursor_pos = self.input.chars().count();
    }
}
KeyCode::Down => {
    if let Some(next) = self.history.down() {
        self.input = next.to_string();
        self.cursor_pos = self.input.chars().count();
    }
}
```

Keep PageUp/PageDown for scrolling (no change needed).

- [ ] **Step 7: Push entry on submit**

In `run.rs`, find where input is submitted (search for `take_input`). After taking input, add:
```rust
if !user_input.trim().is_empty() {
    app.history.push(user_input.clone());
}
app.history.reset();
```

- [ ] **Step 8: Run full tests**

Run: `cargo test --lib`
Expected: All tests pass

- [ ] **Step 9: Commit**

```bash
git add src/tui/history.rs src/tui/mod.rs src/tui/app.rs src/tui/run.rs
git commit -m "feat(tui): add persistent input history with Up/Down navigation"
```

---

## Task 7: Multi-line Input

**Files:**
- Modify: `src/tui/app.rs` (key handling)
- Modify: `src/tui/ui.rs` (dynamic input height, cursor position)
- Modify: `src/tui/run.rs` (kitty protocol, Alt+Enter)

- [ ] **Step 1: Write tests for cursor_row_col helper**

In `app.rs` tests:
```rust
#[test]
fn test_cursor_row_col_single_line() {
    assert_eq!(cursor_row_col("hello", 3), (0, 3));
}

#[test]
fn test_cursor_row_col_multiline() {
    assert_eq!(cursor_row_col("abc\ndef", 5), (1, 1)); // on 'd' in "def"
}

#[test]
fn test_cursor_row_col_at_newline() {
    assert_eq!(cursor_row_col("abc\n", 4), (1, 0)); // start of second line
}

#[test]
fn test_cursor_row_col_empty() {
    assert_eq!(cursor_row_col("", 0), (0, 0));
}

#[test]
fn test_cursor_row_col_cjk() {
    assert_eq!(cursor_row_col("hello\n한글", 7), (1, 1)); // second char of 한글
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib tui::app::tests::test_cursor_row_col -- --nocapture`
Expected: FAIL — function not defined

- [ ] **Step 3: Add cursor_row_col function**

In `app.rs`:
```rust
/// Convert char-based cursor position to (row, col) for multi-line input.
pub fn cursor_row_col(input: &str, cursor_pos: usize) -> (usize, usize) {
    let before_cursor: String = input.chars().take(cursor_pos).collect();
    let row = before_cursor.matches('\n').count();
    let col = before_cursor.chars().rev().take_while(|c| *c != '\n').count();
    (row, col)
}
```

- [ ] **Step 4: Run cursor_row_col tests**

Run: `cargo test --lib tui::app::tests::test_cursor_row_col -- --nocapture`
Expected: All pass

- [ ] **Step 5: Add Alt+Enter / Shift+Enter handling in app.rs**

In `handle_key()`, add before the `KeyCode::Enter` arm:

```rust
KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT)
    || key.modifiers.contains(KeyModifiers::SHIFT) =>
{
    let byte_pos = char_to_byte_pos(&self.input, self.cursor_pos);
    self.input.insert(byte_pos, '\n');
    self.cursor_pos += 1;
}
```

The existing `KeyCode::Enter` arm (submit) already has no modifiers check, so the new arm must come BEFORE it in the match. Restructure:

```rust
KeyCode::Enter => {
    if key.modifiers.contains(KeyModifiers::ALT) || key.modifiers.contains(KeyModifiers::SHIFT) {
        let byte_pos = char_to_byte_pos(&self.input, self.cursor_pos);
        self.input.insert(byte_pos, '\n');
        self.cursor_pos += 1;
    }
    // Else: submit handled by main loop
}
```

Note: The current `handle_key` doesn't process Enter directly — it returns and lets `run.rs` handle submit. So adding newline insertion here for Alt/Shift is clean.

**Important:** In `run.rs` line 395, the submit check `key.code == KeyCode::Enter` must be updated to exclude Alt/Shift modifiers, otherwise Alt+Enter would both insert a newline AND try to submit:

```rust
// run.rs line 395 — add modifier guard:
} else if key.code == KeyCode::Enter
    && !key.modifiers.intersects(KeyModifiers::ALT | KeyModifiers::SHIFT)
    && !app.is_loading && !app.input.is_empty() {
```

- [ ] **Step 6: Update Up/Down for multi-line cursor movement**

In `handle_key()`, modify `KeyCode::Up`:

```rust
KeyCode::Up => {
    let (row, _col) = cursor_row_col(&self.input, self.cursor_pos);
    if row == 0 {
        // First line — use history
        if let Some(prev) = self.history.up(&self.input) {
            self.input = prev.to_string();
            self.cursor_pos = self.input.chars().count();
        }
    } else {
        // Move cursor up one line
        // Find start of current line, then position in previous line
        let lines: Vec<&str> = self.input.split('\n').collect();
        let col = cursor_row_col(&self.input, self.cursor_pos).1;
        let prev_line_len = lines[row - 1].chars().count();
        let target_col = col.min(prev_line_len);
        // Calculate new cursor_pos
        let chars_before_prev_line: usize = lines[..row-1].iter()
            .map(|l| l.chars().count() + 1) // +1 for '\n'
            .sum();
        self.cursor_pos = chars_before_prev_line + target_col;
    }
}
```

Similar for `KeyCode::Down`:

```rust
KeyCode::Down => {
    let lines: Vec<&str> = self.input.split('\n').collect();
    let (row, col) = cursor_row_col(&self.input, self.cursor_pos);
    if row >= lines.len() - 1 {
        // Last line — use history
        if let Some(next) = self.history.down() {
            self.input = next.to_string();
            self.cursor_pos = self.input.chars().count();
        }
    } else {
        // Move cursor down one line
        let next_line_len = lines[row + 1].chars().count();
        let target_col = col.min(next_line_len);
        let chars_before_next_line: usize = lines[..row+1].iter()
            .map(|l| l.chars().count() + 1)
            .sum();
        self.cursor_pos = chars_before_next_line + target_col;
    }
}
```

- [ ] **Step 7: Update Home/End for current line only**

```rust
KeyCode::Home => {
    // Move to start of current line
    let before: String = self.input.chars().take(self.cursor_pos).collect();
    let line_start = before.rfind('\n').map_or(0, |i| {
        before[..i].chars().count() + 1
    });
    self.cursor_pos = line_start;
}
KeyCode::End => {
    // Move to end of current line
    let after: String = self.input.chars().skip(self.cursor_pos).collect();
    let line_end = after.find('\n').unwrap_or(after.chars().count());
    self.cursor_pos += line_end;
}
```

- [ ] **Step 8: Dynamic input height in ui.rs**

Change the layout in `render_in_area()`:

```rust
let input_line_count = app.input.matches('\n').count() + 1;
let max_input_height = (area.height / 3) as usize;
let input_height = (input_line_count.clamp(1, max_input_height) + 2) as u16; // +2 for border

let layout = Layout::vertical([
    Constraint::Min(1),
    Constraint::Length(input_height),
    Constraint::Length(1),
]);
```

- [ ] **Step 9: Fix cursor positioning for multi-line input**

In `render_input()`, update the Normal mode cursor positioning:

```rust
if !app.is_loading {
    let (row, col) = crate::tui::app::cursor_row_col(&app.input, app.cursor_pos);
    let display_col: u16 = app.input.chars()
        .skip(app.cursor_pos - col) // skip to start of current line
        .take(col)
        .map(|c| if c.is_ascii() { 1u16 } else { 2u16 })
        .sum();
    frame.set_cursor_position((area.x + display_col + 1, area.y + 1 + row as u16));
}
```

- [ ] **Step 10: Enable kitty keyboard protocol in run.rs (optional enhancement)**

At terminal setup, after `enable_raw_mode()`:

```rust
// Try to enable kitty keyboard protocol for Shift+Enter detection
let _ = execute!(
    stdout,
    crossterm::event::PushKeyboardEnhancementFlags(
        crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
    )
);
```

At cleanup (before `LeaveAlternateScreen`):

```rust
let _ = execute!(io::stdout(), crossterm::event::PopKeyboardEnhancementFlags);
```

Note: This silently fails on unsupported terminals (returns Ok but no effect), so it's safe to always try. Alt+Enter still works as universal fallback.

- [ ] **Step 11: Run full tests**

Run: `cargo test --lib`
Expected: All tests pass

- [ ] **Step 12: Commit**

```bash
git add src/tui/app.rs src/tui/ui.rs src/tui/run.rs
git commit -m "feat(tui): add multi-line input with Alt+Enter and dynamic height"
```

---

## Task 8: Enhanced Status Bar

**Files:**
- Modify: `src/tui/app.rs` (add fields)
- Modify: `src/tui/ui.rs` (`render_status_bar`)

- [ ] **Step 1: Add fields to App**

In `app.rs`, add to `App` struct:

```rust
pub context_window: u64,
pub permission_mode_label: String,
```

Initialize in `App::new()`:

```rust
context_window: 0,
permission_mode_label: "auto".to_string(),
```

- [ ] **Step 2: Wire context_window from provider in run.rs**

In `run.rs`, after creating the app, if provider exists:

```rust
if let Some(ref p) = provider {
    app.context_window = p.model_info().context_window; // already u64, no cast needed
}
```

And set permission mode label (must cover all 7 variants of PermissionMode):

```rust
app.permission_mode_label = match permission_mode {
    PermissionMode::Auto => "auto",
    PermissionMode::Default => "default",
    PermissionMode::Plan => "plan",
    PermissionMode::AcceptEdits => "accept-edits",
    PermissionMode::BypassPermissions => "bypass",
    PermissionMode::DontAsk => "dont-ask",
    PermissionMode::Bubble => "bubble",
}.to_string();
```

- [ ] **Step 3: Rewrite render_status_bar in ui.rs**

Replace the existing `render_status_bar` function:

```rust
fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let bg = Style::default().bg(Color::DarkGray);
    let sep = Span::styled(" · ", bg.fg(Color::Indexed(240))); // subtle gray

    // Model
    let model_span = Span::styled(
        format!(" {}", app.active_model()),
        bg.fg(Color::Cyan).bold(),
    );

    // Permission mode
    let perm_span = Span::styled(&app.permission_mode_label, bg.fg(Color::White));

    // CWD (with ~ substitution)
    let home = std::env::var("HOME").unwrap_or_default();
    let cwd = std::env::current_dir()
        .map(|p| {
            let s = p.to_string_lossy().to_string();
            if !home.is_empty() { s.replacen(&home, "~", 1) } else { s }
        })
        .unwrap_or_default();
    let cwd_span = Span::styled(&cwd, bg.fg(Color::White));

    // Context usage
    let ctx_pct = if app.context_window > 0 {
        (app.token_count as f64 / app.context_window as f64 * 100.0) as u16
    } else { 0 };
    let ctx_color = match ctx_pct {
        0..=49 => Color::Green,
        50..=79 => Color::Yellow,
        _ => Color::Red,
    };
    let ctx_span = Span::styled(format!("ctx: {}%", ctx_pct), bg.fg(ctx_color));

    // Right side
    let right = format!("tokens: {} | ${:.4} ", app.token_count, app.session_cost);
    let right_span = Span::styled(&right, bg.fg(Color::White));

    // Build left side, with overflow handling
    let right_len = right.len();
    let available = (area.width as usize).saturating_sub(right_len);

    let model_str = format!(" {}", app.active_model());
    let perm_str = &app.permission_mode_label;
    let ctx_str = format!("ctx: {}%", ctx_pct);

    // Calculate what fits
    let full_left_len = model_str.len() + 3 + perm_str.len() + 3 + cwd.len() + 3 + ctx_str.len();

    let mut left_spans: Vec<Span> = Vec::new();
    if full_left_len <= available {
        // Full: model · perm · cwd · ctx%
        left_spans.extend([model_span, sep.clone(), perm_span, sep.clone(), cwd_span, sep.clone(), ctx_span]);
    } else if model_str.len() + 3 + perm_str.len() + 3 + ctx_str.len() <= available {
        // Drop CWD
        left_spans.extend([model_span, sep.clone(), perm_span, sep.clone(), ctx_span]);
    } else if model_str.len() + 3 + ctx_str.len() <= available {
        // Drop perm
        left_spans.extend([model_span, sep.clone(), ctx_span]);
    } else {
        // Minimum: model only
        left_spans.push(model_span);
    }

    let left_len: usize = left_spans.iter().map(|s| s.content.len()).sum();
    let middle_width = (area.width as usize).saturating_sub(left_len + right_len);
    let middle = Span::styled(" ".repeat(middle_width), bg);

    let mut all_spans = left_spans;
    all_spans.push(middle);
    all_spans.push(right_span);

    frame.render_widget(Paragraph::new(Line::from(all_spans)), area);
}
```

- [ ] **Step 4: Run full tests**

Run: `cargo test --lib`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add src/tui/app.rs src/tui/ui.rs src/tui/run.rs
git commit -m "feat(tui): enhance status bar with permission mode, CWD, context usage"
```

---

## Task 9: Blockquote Rendering Fix

The MarkdownRenderer from Task 1 handles blockquotes via style stack but doesn't add the `│ ` visual prefix. Fix this.

**Files:**
- Modify: `src/tui/markdown.rs`

- [ ] **Step 1: Write test for blockquote visual prefix**

```rust
#[test]
fn test_blockquote_prefix() {
    let lines = parse_markdown("> quoted text");
    assert!(lines[0].spans.iter().any(|s| s.content.contains('│')));
}
```

- [ ] **Step 2: Add blockquote tracking**

Add field: `blockquote_depth: usize` to `MarkdownRenderer` (supports nesting).

In Task 1's `handle_start(Tag::BlockQuote(_))`: remove the style_stack push (italic+DarkGray). Instead increment `blockquote_depth`.
In Task 1's `handle_end(TagEnd::BlockQuote(_))`: remove the style_stack pop. Instead decrement `blockquote_depth`.

The blockquote style is applied via `flush_line()` prefix, not the style stack — this avoids the two mechanisms getting out of sync:

```rust
fn flush_line(&mut self) {
    let mut spans = std::mem::take(&mut self.current_line);
    if !spans.is_empty() {
        if self.blockquote_depth > 0 {
            let prefix = "│ ".repeat(self.blockquote_depth);
            spans.insert(0, Span::styled(prefix, Style::new().fg(Color::DarkGray)));
        }
        self.lines.push(Line::from(spans));
    }
}
```

Text inside blockquotes is rendered in italic via `merged_style()` — push italic onto style_stack in `handle_start(BlockQuote)` and pop in `handle_end(BlockQuote)`:
```rust
// handle_start:
Tag::BlockQuote(_) => {
    self.flush_line();
    self.blockquote_depth += 1;
    self.style_stack.push(Style::new().italic());
}
// handle_end:
TagEnd::BlockQuote(_) => {
    self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
    self.style_stack.pop();
    self.flush_line();
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib tui::markdown -- --nocapture`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/tui/markdown.rs
git commit -m "feat(tui): add blockquote visual prefix with dim border"
```

---

## Task 10: Markdown Render Cache

**Files:**
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/run.rs`

The cache lives as a local variable in the render loop in `run.rs`, passed into render functions. This avoids changing `render`'s `&App` signature to `&mut App` (which would cascade through all render functions and conflict with `terminal.draw` closure patterns).

- [ ] **Step 1: Add cache type and pass-through**

In `ui.rs`, add:
```rust
use std::collections::HashMap;

pub type MarkdownCache = HashMap<usize, (usize, Vec<Line<'static>>)>;
```

Change `render` and `render_in_area` signatures to accept `&mut MarkdownCache`:
```rust
pub fn render(frame: &mut Frame, app: &App, md_cache: &mut MarkdownCache) -> Vec<HitArea>
pub fn render_in_area(frame: &mut Frame, app: &App, area: Rect, _with_command: bool, md_cache: &mut MarkdownCache) -> Vec<HitArea>
fn render_conversation(lines: &mut Vec<Line<'static>>, app: &App, _width: u16, md_cache: &mut MarkdownCache) -> Vec<HitArea>
```

In `render_conversation`, assistant rendering:
```rust
DisplayMessage::Text { role: TextRole::Assistant, content } => {
    lines.push(Line::from(""));
    let len = content.len();
    let cached = md_cache.entry(msg_idx)
        .and_modify(|e| {
            if e.0 != len {
                e.0 = len;
                e.1 = crate::tui::markdown::parse_markdown(content);
            }
        })
        .or_insert_with(|| {
            (len, crate::tui::markdown::parse_markdown(content))
        });
    for line in &cached.1 {
        let mut prefixed: Vec<Span<'static>> = vec![Span::raw("  ")];
        prefixed.extend(line.spans.clone());
        lines.push(Line::from(prefixed));
    }
}
```

- [ ] **Step 2: Update run.rs to create and pass cache**

In `run.rs`, before the main loop, add:
```rust
let mut md_cache = ui::MarkdownCache::new();
```

Update the `terminal.draw` closure:
```rust
frame_hit_areas = ui::render(frame, &app, &mut md_cache);
```

Clear cache on `/clear` command (when `app.messages` is emptied):
```rust
md_cache.clear();
```

- [ ] **Step 3: Run tests**

Run: `cargo test --lib`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add src/tui/ui.rs src/tui/run.rs
git commit -m "perf(tui): add markdown render cache to avoid re-parsing unchanged messages"
```

---

## Task 11: Final Integration Test & Cleanup

**Files:**
- All modified files

- [ ] **Step 1: Run full test suite**

Run: `cargo test --lib`
Expected: All tests pass (595+ original + new tests)

- [ ] **Step 2: Run cargo clippy**

Run: `cargo clippy -- -D warnings 2>&1 | head -30`
Expected: No warnings

- [ ] **Step 3: Verify compilation**

Run: `cargo build --release 2>&1 | tail -5`
Expected: Clean build

- [ ] **Step 4: Commit any cleanup**

```bash
git add -A
git commit -m "chore: clippy fixes and cleanup for TUI polish"
```
