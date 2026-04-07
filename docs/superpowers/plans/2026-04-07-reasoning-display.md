# Reasoning/Thinking Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show LLM thinking/reasoning in TUI — progress indicator during thinking, collapsible block after completion.

**Architecture:** Add ThinkingDelta event forwarding from engine to TUI, new DisplayMessage::Thinking variant, render with spinner/collapse pattern matching existing ToolCall/Agent.

**Tech Stack:** Rust, ratatui

**Spec:** `docs/superpowers/specs/2026-04-07-reasoning-display-design.md`

---

## Task 1: ThinkingDelta Event + DisplayMessage::Thinking

**Files:**
- Modify: `src/query/types.rs`
- Modify: `src/query/engine.rs`
- Modify: `src/tui/app.rs`
- Modify: `src/entrypoints/print.rs`

- [ ] **Step 1: Add ThinkingDelta to QueryEvent**

In `src/query/types.rs`, add to the `QueryEvent` enum:
```rust
/// Streaming thinking/reasoning content from LLM
ThinkingDelta { text: String },
```

- [ ] **Step 2: Send ThinkingDelta from engine.rs**

In `src/query/engine.rs`, in the stream loop where `ThinkingDelta` is handled (~line 216):
```rust
ContentDelta::ThinkingDelta { thinking } => {
    reasoning_content.push_str(&thinking);
    let _ = event_tx.send(QueryEvent::ThinkingDelta { text: thinking });
}
```

- [ ] **Step 3: Add DisplayMessage::Thinking to app.rs**

```rust
Thinking {
    content: String,
    token_count: u64,
    is_complete: bool,
    collapsed: bool,
},
```

- [ ] **Step 4: Handle ThinkingDelta in app.rs handle_query_event**

```rust
QueryEvent::ThinkingDelta { text } => {
    // Find open Thinking message (last one, not complete)
    let found = self.messages.iter_mut().rev().find(|m| {
        matches!(m, DisplayMessage::Thinking { is_complete: false, .. })
    });
    if let Some(DisplayMessage::Thinking { content, token_count, .. }) = found {
        content.push_str(&text);
        *token_count = (content.len() as u64) / 4;
    } else {
        // Create new Thinking message
        let tc = (text.len() as u64) / 4;
        self.messages.push(DisplayMessage::Thinking {
            content: text,
            token_count: tc,
            is_complete: false,
            collapsed: true,
        });
    }
}
```

- [ ] **Step 5: Mark thinking complete on TextDelta**

In the existing `TextDelta` handler in app.rs, before appending text, mark any open thinking as complete:
```rust
QueryEvent::TextDelta(text) => {
    // Mark any open thinking as complete
    for msg in self.messages.iter_mut().rev() {
        if let DisplayMessage::Thinking { is_complete, .. } = msg {
            if !*is_complete {
                *is_complete = true;
            }
            break;
        }
    }
    // ... existing TextDelta handling
}
```

- [ ] **Step 6: Clean up empty thinking on QueryCompleted**

In `QueryCompleted` handler, remove empty thinking messages:
```rust
self.messages.retain(|m| {
    !matches!(m, DisplayMessage::Thinking { content, .. } if content.is_empty())
});
```

Also mark any remaining open thinking as complete.

- [ ] **Step 7: Ignore ThinkingDelta in print.rs**

In `src/entrypoints/print.rs`, add to the event match:
```rust
QueryEvent::ThinkingDelta { .. } => {}
```

- [ ] **Step 8: Fix mark_pending_tools_done and other DisplayMessage patterns**

Any exhaustive match on `DisplayMessage` needs a `Thinking { .. }` arm. Check `mark_pending_tools_done`, `handle_cancel`, and any other match blocks.

- [ ] **Step 9: Build and test**

Run: `cargo build 2>&1 | grep "^error" | head -20` → no errors
Run: `cargo test --lib -- 2>&1 | tail -5` → all pass
Run: `cargo fmt`

- [ ] **Step 10: Commit**

```
feat(tui): add ThinkingDelta event forwarding and DisplayMessage::Thinking
```

---

## Task 2: Thinking Rendering

**Files:**
- Modify: `src/tui/ui.rs`
- Modify: `src/tui/run.rs` (click handler)

- [ ] **Step 1: Add Thinking render block in ui.rs**

In `render_conversation()`, add a match arm for `DisplayMessage::Thinking`. Place it alongside the existing ToolCall/Agent arms:

```rust
DisplayMessage::Thinking { content, token_count, is_complete, collapsed } => {
    let token_str = format_tokens(*token_count);
    
    if !is_complete {
        // In progress: spinner
        let spinners = ['◐', '◓', '◑', '◒'];
        let elapsed = app.query_start_time.map(|t| t.elapsed()).unwrap_or_default();
        let spinner = spinners[(elapsed.as_millis() / 200) as usize % spinners.len()];
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", spinner), Style::default().fg(Color::Yellow)),
            Span::styled("Thinking… ", Style::default().fg(Color::Yellow)),
            Span::styled(format!("({})", token_str), Style::default().fg(Color::DarkGray)),
        ]));
    } else if *collapsed {
        // Completed, collapsed (clickable)
        let header_line_idx = lines.len();
        lines.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(Color::DarkGray)),
            Span::styled("Thinking ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("({})", token_str), Style::default().fg(Color::DarkGray)),
        ]));
        hit_areas.push(HitArea { y: header_line_idx as u16, message_idx: msg_idx });
    } else {
        // Completed, expanded
        let header_line_idx = lines.len();
        lines.push(Line::from(vec![
            Span::styled("▼ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Thinking ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("({})", token_str), Style::default().fg(Color::DarkGray)),
        ]));
        hit_areas.push(HitArea { y: header_line_idx as u16, message_idx: msg_idx });
        // Content lines (max 20)
        for line in content.lines().take(20) {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(Color::DarkGray)),
                Span::styled(line.to_string(), Style::default().fg(Color::DarkGray)),
            ]));
        }
        let total_lines = content.lines().count();
        if total_lines > 20 {
            lines.push(Line::from(Span::styled(
                format!("  │ ... {} more lines", total_lines - 20),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
}
```

- [ ] **Step 2: Add click handler for Thinking in run.rs**

In the mouse click handler, add Thinking toggle alongside existing ToolCall/Agent toggles:
```rust
DisplayMessage::Thinking { collapsed, is_complete, .. } if *is_complete => {
    *collapsed = !*collapsed;
}
```

- [ ] **Step 3: Handle Thinking in message grouping**

In the pre-scan grouping logic, Thinking messages should NOT be grouped with ToolCalls. Add a break condition if needed.

- [ ] **Step 4: Build and test**

Run: `cargo build 2>&1 | grep "^error"` → no errors
Run: `cargo test --lib -- 2>&1 | tail -5` → all pass
Run: `cargo fmt`

- [ ] **Step 5: Commit**

```
feat(tui): render thinking blocks with spinner, collapse, and content display
```

---

## Task 3: Smoke Test

- [ ] **Step 1: Build release**
```bash
cargo build --release 2>&1 | tail -3
```

- [ ] **Step 2: Test with thinking model**

Use a thinking model (e.g., Gemma4 with reasoning enabled, or Qwen3). Verify:
- During thinking: `◐ Thinking… (Nk tokens)` spinner appears
- After response: `● Thinking (Nk tokens)` collapsed block above response
- Click on thinking block: expands to show thinking content
- Click again: collapses back

- [ ] **Step 3: Test with non-thinking model**

Use a non-thinking model. Verify:
- No thinking block appears (no ThinkingDelta events → no Thinking message)
- Everything works as before

- [ ] **Step 4: Commit**
```
chore: verified reasoning display
```
