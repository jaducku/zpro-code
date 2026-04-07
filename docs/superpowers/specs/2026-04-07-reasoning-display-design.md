# Reasoning/Thinking Display Design

Show LLM thinking/reasoning content in the TUI — progress indicator during thinking, collapsible block to view thinking content after completion.

## Problem

- `ThinkingDelta` events are collected in engine.rs but never sent to TUI
- User has no visibility into model thinking (just sees "Thinking..." with no detail)
- Thinking content is silently discarded after the response

## Reference UI

```
◐ Thinking… (1.2k tokens)              ← during thinking
Hello! Let me help you with that...     ← assistant response streaming

● Thinking (2.4k tokens)                ← after completion (collapsed, clickable)
Hello! Let me help you with that...

▼ Thinking (2.4k tokens)                ← expanded
  │ Let me analyze the problem step by step...
  │ First, I need to check the data model...
  │ The key issue is that...
  │ ...
Hello! Let me help you with that...
```

---

## Section 1: Events

### New QueryEvent variants

```rust
/// Streaming thinking content from LLM
ThinkingDelta { text: String },
```

No separate `ThinkingComplete` event needed — thinking ends when the first `TextDelta` arrives or when the stream ends. Detect completion in the TUI by checking `is_complete` flag.

### engine.rs changes

In the stream loop, when `ThinkingDelta` is received, forward to TUI:
```rust
ContentDelta::ThinkingDelta { thinking } => {
    reasoning_content.push_str(&thinking);
    let _ = event_tx.send(QueryEvent::ThinkingDelta { text: thinking });
}
```

After the stream loop, when text_content starts being built (meaning thinking is done), mark thinking as complete. This is signaled by detecting that ThinkingDelta was followed by TextDelta or stream end — handled in app.rs.

---

## Section 2: Data Model

### New DisplayMessage variant

```rust
DisplayMessage::Thinking {
    content: String,        // accumulated thinking text
    token_count: u64,       // estimated thinking tokens (chars / 4)
    is_complete: bool,      // true when thinking phase ends
    collapsed: bool,        // default: true (collapsed)
}
```

### Event handling in app.rs

**ThinkingDelta { text }:**
1. Find last `Thinking` message in `self.messages` (reverse scan)
2. If not found or last one is `is_complete == true`, create new `Thinking` message
3. Append text to `content`, increment `token_count` by `text.len() / 4`

**TextDelta (existing handler):**
- Before appending to assistant message, mark any open `Thinking` message as `is_complete = true`

**QueryCompleted:**
- If last `Thinking` message has empty content, remove it (empty thinking block from Gemma4 disabled mode)

---

## Section 3: Rendering (ui.rs)

### Layout

Thinking message renders inline in the message list, positioned before the assistant response (natural order from events).

### States

**In progress (`!is_complete`):**
```
◐ Thinking… ({token_count} tokens)
```
- `◐` spinner in Yellow (rotating: ◐ ◓ ◑ ◒)
- "Thinking…" in Yellow
- Token count in DarkGray

**Completed, collapsed (`is_complete && collapsed`):**
```
● Thinking ({token_count} tokens)
```
- `●` in DarkGray
- Clickable (HitArea registered)

**Completed, expanded (`is_complete && !collapsed`):**
```
▼ Thinking ({token_count} tokens)
  │ {line 1}
  │ {line 2}
  │ ...
  │ {line N}  (max 20 lines, then truncated)
```
- `▼` in DarkGray
- Content lines prefixed with `  │ ` in DarkGray
- Max 20 lines displayed; longer content truncated

### Interaction

- Click on Thinking header toggles `collapsed`
- Uses existing `hit_areas` pattern (same as ToolCall/Agent)

---

## Section 4: File Changes

| File | Changes |
|------|---------|
| `src/query/types.rs` | Add `ThinkingDelta { text: String }` to QueryEvent |
| `src/query/engine.rs` | Send `ThinkingDelta` event in stream loop |
| `src/tui/app.rs` | Add `DisplayMessage::Thinking`, handle ThinkingDelta/TextDelta events |
| `src/tui/ui.rs` | Render Thinking message (spinner, collapsed, expanded) |
| `src/tui/run.rs` | Handle Thinking click in mouse handler |
| `src/entrypoints/print.rs` | Ignore `ThinkingDelta` (no display in print mode) |

### Not changed

- OpenAI provider (already produces ThinkingDelta via ContentDelta::ThinkingDelta)
- Reasoning fallback logic in engine.rs (still converts reasoning to text if no text content)
- Compact system (thinking not included in compact messages)
