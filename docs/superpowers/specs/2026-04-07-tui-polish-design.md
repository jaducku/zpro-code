# TUI Polish Design Spec

**Date:** 2026-04-07
**Goal:** Bring atom-code-rs TUI rendering to feature parity with the original TypeScript Claude Code CLI.

## Overview

8 improvements to the TUI layer, adding markdown rendering, syntax highlighting, table support, hyperlinks, input history, multi-line input, enhanced status bar, and improved tool result display.

**New files:** `src/tui/markdown.rs`, `src/tui/history.rs`
**Modified files:** `src/tui/ui.rs`, `src/tui/app.rs`, `src/tui/run.rs`, `src/tui/mod.rs`

---

## 1. Markdown Rendering Pipeline

### Problem
Assistant text is rendered as plain text via `Line::from(Span::raw(line))`. No bold, italic, headers, lists, blockquotes, or inline code styling.

### Design

New module `src/tui/markdown.rs` with entry point:

```rust
pub fn parse_markdown(text: &str) -> Vec<Line<'static>>
```

Uses `pulldown_cmark::Parser` (already in Cargo.toml) to tokenize markdown, then converts the event stream into `Vec<Line>` with styled `Span`s.

### Markdown → ratatui Style Mapping

| Markdown Element | ratatui Style |
|-----------------|---------------|
| `**bold**` | `Style::new().bold()` |
| `*italic*` | `Style::new().italic()` |
| `` `inline code` `` | `Style::new().fg(Color::Yellow)` |
| `# H1` | Bold + Italic + Underline |
| `## H2` | Bold |
| `### H3+` | Bold |
| `- unordered list` | `  - ` prefix, nested indent (`  ` per depth) |
| `1. ordered list` | `  1. ` prefix, nested indent |
| `> blockquote` | Dim `│ ` prefix + italic text |
| `[text](url)` | Blue + underline (see section 4) |
| `---` (thematic break) | `─` repeated to terminal width |

### Style Stack

Markdown has nested formatting (e.g., `**bold _and italic_**`). Maintain a style stack:

```rust
struct MarkdownRenderer {
    style_stack: Vec<Style>,   // push on Start, pop on End
    current_line: Vec<Span>,   // accumulate spans for current line
    lines: Vec<Line>,          // completed lines
    list_depth: usize,         // nesting level for lists
    list_counters: Vec<Option<u64>>,  // None=unordered, Some(n)=ordered
}
```

When encountering `Event::Start(Tag::Strong)`, push bold onto stack. On `Event::Text(s)`, emit `Span::styled(s, merged_style())`. On `Event::End(TagEnd::Strong)`, pop.

**Inline code:** pulldown-cmark emits inline code as `Event::Code(CowStr)` (a leaf event, NOT a Start/End pair). Handle directly:
```rust
Event::Code(text) => {
    self.current_line.push(Span::styled(text.into_string(), Style::new().fg(Color::Yellow)));
}
```

### Integration Point

In `ui.rs`, replace the assistant message rendering (currently lines 189-196):

```rust
// Before:
for line_text in content.lines() {
    lines.push(Line::from(vec![Span::raw(format!("  {}", line_text))]));
}

// After:
let md_lines = markdown::parse_markdown(content);
for line in md_lines {
    let mut prefixed = vec![Span::raw("  ")];
    prefixed.extend(line.spans);
    lines.push(Line::from(prefixed));
}
```

### Caching

During streaming, assistant content changes with every `TextDelta`. Re-parsing on every render is wasteful.

Keep the cache in the rendering layer, not in `DisplayMessage` (to maintain separation of concerns):

```rust
// In ui.rs or a render_cache module
struct MarkdownCache {
    entries: HashMap<usize, (usize, Vec<Line<'static>>)>,  // msg_index → (content_len, parsed)
}
```

On render: compare `content.len()` with cached length. If equal, reuse. If not, re-parse and update cache.

**Streaming optimization:** During active streaming (`is_loading` = true), only re-parse the last message. All other messages use their cache. Start with this simple approach — add hash-based caching only if profiling shows a bottleneck.

---

## 2. Code Block Syntax Highlighting

### Problem
Code blocks display as plain text despite `syntect` being in Cargo.toml.

### Design

Within `markdown.rs`, when `Event::Start(Tag::CodeBlock(kind))` is encountered:

```rust
fn highlight_code(code: &str, lang: &str) -> Vec<Line<'static>>
```

### Implementation

```rust
use syntect::highlighting::{ThemeSet, Style as SyntectStyle};
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

use std::sync::LazyLock;  // stable since Rust 1.80

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME: LazyLock<syntect::highlighting::Theme> = LazyLock::new(|| {
    let ts = ThemeSet::load_defaults();
    ts.themes["base16-ocean.dark"].clone()
});
```

For each line of code:
1. `HighlightLines::highlight_line(line, &SYNTAX_SET)` returns `Result<Vec<(SyntectStyle, &str)>>` — on error, fall back to plain text (unstyled)
2. Convert: `SyntectStyle.foreground` → `Color::Rgb(r, g, b)`
3. Wrap in `Span::styled(text, Style::new().fg(color))`

### Code Block Visual Treatment

```
  ┌─ rust ─────────────────────────┐
  │ fn main() {                    │
  │     println!("hello");         │
  │ }                              │
  └────────────────────────────────┘
```

- Language label in top border (DarkGray text)
- Left border `│ ` (DarkGray)
- If lang not recognized by syntect, fall back to plain text (no highlighting, but still bordered)
- Border width: min(content_max_line_width + 4, terminal_width - 4)

### Language Detection

`pulldown_cmark::CodeBlockKind::Fenced(lang)` provides the language. Map common aliases:
- `js` → `javascript`, `ts` → `typescript`, `py` → `python`, `sh`/`bash` → `shell`
- Use `SYNTAX_SET.find_syntax_by_token(lang)` which handles most aliases already

---

## 3. Table Rendering

### Problem
Markdown tables render as plain text.

### Design

When pulldown-cmark emits `Table` events, collect all cells in a two-pass approach:

**Pass 1 — Collect:**
```rust
struct TableBuilder {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    alignments: Vec<Alignment>,
}
```

Accumulate cell text between `Start(TableHead/TableRow/TableCell)` and `End(...)` events.

**Pass 2 — Render:**

```rust
fn render_table(builder: TableBuilder, max_width: u16) -> Vec<Line<'static>>
```

1. Calculate column widths: `max(header_len, max_cell_len)` per column
2. If total exceeds `max_width`: proportionally shrink columns, minimum 3 chars
3. If still doesn't fit at minimum: switch to vertical format (`key: value`)
4. Generate lines with box-drawing characters:

```
┌──────────┬────────┬───────┐
│ Name     │ Status │ Note  │
├──────────┼────────┼───────┤
│ session  │ done   │ -     │
│ markdown │ wip    │ here  │
└──────────┴────────┴───────┘
```

**Alignment:** Apply padding per `Alignment`:
- Left: `text + spaces`
- Right: `spaces + text`
- Center: balanced padding

**Vertical fallback** (when terminal too narrow):
```
──────────
Name:   session
Status: done
Note:   -
──────────
Name:   markdown
Status: wip
Note:   here
```

---

## 4. Hyperlinks

### Problem
URLs in markdown render as plain text with no visual distinction.

### Design

**Approach:** Visual-only styling (blue + underline). ratatui 0.29 does not natively support OSC 8 terminal hyperlinks, so we style them visually without click support.

When `Event::Start(Tag::Link { dest_url, .. })` is encountered:
- Push `Style::new().fg(Color::Blue).underlined()` onto style stack
- Store `dest_url` for potential future use
- On `Event::End(TagEnd::Link)`, pop style

**Auto-detection patterns** (applied as post-processing on plain text spans):
- GitHub issues: `owner/repo#123` → Blue + underline
- URLs: `https://...` in plain text → Blue + underline

This keeps it simple and forward-compatible. If ratatui adds OSC 8 support later, we can wire in the stored `dest_url`.

---

## 5. Input History

### Problem
No input history. Up/Down arrows scroll messages instead of navigating previous inputs.

### Design

New module `src/tui/history.rs`:

```rust
pub struct InputHistory {
    entries: Vec<String>,
    cursor: Option<usize>,      // None = not browsing, Some(i) = viewing entries[i]
    stashed: Option<String>,    // Current input saved when browsing starts
    max_entries: usize,         // 500
    file_path: PathBuf,         // Uses global_config_dir()/history.json (same dir as config.json)
}

impl InputHistory {
    pub fn load(file_path: &Path) -> Self;
    pub fn push(&mut self, entry: String);  // Add + save to file
    pub fn up(&mut self, current_input: &str) -> Option<&str>;
    pub fn down(&mut self) -> Option<&str>;  // Returns stashed at end
    pub fn reset(&mut self);  // Exit browsing mode
}
```

### File Format

```json
["first input", "second input", "most recent"]
```

Simple JSON array. Load on startup, append+flush on each submit.

### Key Binding Changes

**Current:**
- Up/Down → message scroll (±3 lines)
- PageUp/PageDown → message scroll (±20 lines)

**After:**
- Up/Down → input history (when input area focused)
- PageUp/PageDown → message scroll
- Scroll with mouse wheel remains unchanged

**Multi-line interaction (see section 6):** When input has multiple lines:
- Up at first line → history previous
- Down at last line → history next
- Up/Down at middle lines → move cursor between lines

### Integration

In `app.rs`, add `history: InputHistory` field. On `KeyCode::Up`:
```rust
if self.input_mode == InputMode::Normal {
    if let Some(prev) = self.history.up(&self.input) {
        self.input = prev.to_string();
        self.cursor_pos = self.input.len();
    }
}
```

On Enter submit:
```rust
if !self.input.trim().is_empty() {
    self.history.push(self.input.clone());
}
self.history.reset();
```

---

## 6. Multi-line Input

### Problem
Input is single-line only. No way to enter multi-line prompts.

### Design

### Key Bindings
- `Enter` → submit input
- `Alt+Enter` → insert `\n` at cursor position (primary binding, works in all terminals)
- `Shift+Enter` → insert `\n` (only works when kitty keyboard protocol is active)

**Terminal compatibility:** `Shift+Enter` is indistinguishable from `Enter` in standard terminal mode (crossterm limitation). The kitty keyboard protocol (`PushKeyboardEnhancementFlags(DISAMBIGUATE_ESCAPE_CODES)`) enables it but is not supported by all terminals (e.g., macOS Terminal.app). Therefore:

1. Enable kitty protocol in `run.rs` on startup with `supports_keyboard_enhancement()` check
2. `Alt+Enter` always works as universal fallback
3. Both bindings insert `\n`

### Input Field Changes

`app.rs` input handling:
```rust
// Alt+Enter (universal) or Shift+Enter (kitty protocol)
KeyEvent { code: KeyCode::Enter, modifiers, .. }
    if modifiers.contains(KeyModifiers::ALT) || modifiers.contains(KeyModifiers::SHIFT) =>
{
    self.input.insert(byte_pos, '\n');
    self.cursor_pos += 1;
}
```

### Cursor Position Tracking

Current `cursor_pos: usize` counts characters. This still works — `\n` is a character. But for rendering we need row/col:

```rust
fn cursor_row_col(input: &str, cursor_pos: usize) -> (usize, usize) {
    let before_cursor: String = input.chars().take(cursor_pos).collect();
    let row = before_cursor.matches('\n').count();
    // Use char count (not byte count) for column — consistent with cursor_pos being char-based
    let col = before_cursor.chars().rev().take_while(|c| *c != '\n').count();
    (row, col)
}
```

Note: For display-width-accurate cursor positioning with CJK characters, use `unicode_width::UnicodeWidthChar` on the column chars, matching the existing CJK width logic in `render_input`.

### Dynamic Input Height

In `ui.rs` layout:
```rust
let input_line_count = app.input.matches('\n').count() + 1;
let max_input_height = (area.height / 3) as usize;
let input_height = input_line_count.clamp(1, max_input_height) + 2;  // +2 for border

Layout::vertical([
    Constraint::Min(1),                          // messages
    Constraint::Length(input_height as u16),      // input (dynamic)
    Constraint::Length(1),                        // status bar
])
```

### Rendering

`Paragraph::new(input_text)` handles `\n` automatically in ratatui. Cursor placement uses `cursor_row_col()` to set cursor at the correct (x, y) within the input area.

### Arrow Key Behavior Within Multi-line Input

- `Left/Right`: Move cursor ±1 char (crosses line boundaries naturally)
- `Up`: If cursor is on first line → history. Otherwise move cursor up one line (same column or end of shorter line).
- `Down`: If cursor is on last line → history next. Otherwise move cursor down one line.
- `Home/End`: Move to start/end of current line (not entire input)

---

## 7. Enhanced Status Bar

### Problem
Status bar only shows model name and token/cost. Missing: permission mode, CWD, context usage.

### Design

**Layout:**
```
 sonnet · auto · ~/project  ·  ctx: 45%          tokens: 12.3k | $0.0342
 ├─────┘  ├───┘  ├─────────┘   ├──────┘          ├─────────────────────┘
 model    perm   cwd           context%           tokens + cost (right-aligned)
```

### App State Additions

```rust
// Already exist:
pub model: String,
pub token_count: u64,
pub session_cost: f64,

// Add:
pub context_window: u64,     // from provider.model_info().context_window
pub permission_mode: String,  // "auto" | "ask" | "plan" — from settings/config
```

`context_window` is already dynamically fetched from provider. Just store it in App on query start.

### Rendering

```rust
fn render_status_bar(area: Rect, frame: &mut Frame, app: &App) {
    let home = std::env::var("HOME").unwrap_or_default();
    let cwd = std::env::current_dir()
        .map(|p| {
            let s = p.to_string_lossy().to_string();
            if !home.is_empty() { s.replacen(&home, "~", 1) } else { s }
        })
        .unwrap_or_default();

    let ctx_pct = if app.context_window > 0 {
        (app.token_count as f64 / app.context_window as f64 * 100.0) as u16
    } else { 0 };

    let ctx_color = match ctx_pct {
        0..=49 => Color::Green,
        50..=79 => Color::Yellow,
        _ => Color::Red,
    };

    let sep = Span::styled(" · ", Style::new().fg(Color::DarkGray));

    let left = vec![
        Span::styled(&app.model, Style::new().fg(Color::Cyan).bold()),
        sep.clone(),
        Span::styled(&app.permission_mode, Style::new().fg(Color::White)),
        sep.clone(),
        Span::styled(&cwd, Style::new().fg(Color::White)),
        sep.clone(),
        Span::styled(format!("ctx: {}%", ctx_pct), Style::new().fg(ctx_color)),
    ];

    let right = format!("tokens: {} | ${:.4}", format_token_count(app.token_count), app.session_cost);

    // Right-align: fill middle with spaces
}
```

### Width Overflow

If terminal is too narrow, drop items right-to-left from the left section:
1. Full: `model · perm · cwd · ctx%`
2. Drop CWD: `model · perm · ctx%`
3. Drop perm: `model · ctx%`
4. Minimum: `model`

Right section (tokens/cost) always shown.

---

## 8. Tool Result Display Improvement

### Problem
Tool results are limited to 5-line preview, and collapsed by default. Users must click to see results.

### Design

### Changes
1. **Default collapsed:** `true` → `false` (expanded by default)
2. **Remove 5-line limit:** Show full result when expanded
3. **Toggle behavior:** Click header → collapsed/expanded (binary toggle, using existing `HitArea` click infrastructure in `ui.rs`/`run.rs`)

### Code Changes

In `app.rs` where ToolCall messages are created:
```rust
// Change default:
collapsed: false,  // was: true
```

In `ui.rs` where tool results are rendered (lines 233-250):
```rust
// Remove max_preview logic:
if !collapsed {
    if let Some(ref result_text) = result {
        for line_text in result_text.lines() {
            lines.push(Line::from(vec![
                Span::styled("    │ ", Style::new().fg(Color::DarkGray)),
                Span::styled(line_text.to_string(), Style::new().fg(Color::DarkGray)),
            ]));
        }
    }
}
```

Same change for Agent result display.

---

## Dependencies

Already in Cargo.toml:
- `pulldown-cmark` — markdown parsing
- `syntect` — syntax highlighting
- `serde` / `serde_json` — history file serialization

No new dependencies needed.

## Testing Strategy

- **markdown.rs:** Unit tests for each markdown element (bold, italic, code, list, table, link, mixed nesting)
- **history.rs:** Unit tests for push/up/down/reset cycle, file persistence, max_entries cap
- **Integration:** Manual TUI testing with sample markdown content
- **Edge cases:** Empty code blocks, deeply nested lists, tables wider than terminal, zero-width terminal, CJK characters in tables
