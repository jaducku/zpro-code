# TUI Scroll & Inline UX Design

**Date:** 2026-04-05
**Status:** Approved
**Goal:** Fix 4 TUI issues — scroll support, message truncation, permission UX duplication, drawer-to-overlay conversion

## Problems

1. **Long messages truncated** — auto-scroll always jumps to bottom; top of long messages invisible
2. **No scroll** — `scroll_offset` exists in App state but `render_messages` ignores it
3. **Permission UX duplication** — permission asks are pushed as DisplayMessage AND shown in input area
4. **Drawer layout** — TUI commands render in right-side drawer (screen split), should be dropdown overlay above input

## Design

### 1. Scroll Support

**File:** `src/tui/ui.rs` — `render_messages()`

Current (broken):
```rust
let skip = lines.len().saturating_sub(visible_height);
let paragraph = Paragraph::new(lines).scroll((skip as u16, 0));
```

New:
```rust
let total = lines.len();
let max_scroll = total.saturating_sub(visible_height);
// scroll_offset 0 = bottom (auto-scroll), higher = further up
let effective_offset = app.scroll_offset.min(max_scroll as u16);
let skip = max_scroll.saturating_sub(effective_offset as usize);
let paragraph = Paragraph::new(lines).scroll((skip as u16, 0));
```

**File:** `src/tui/app.rs` — `handle_key()` scroll behavior

Current: Up/Down directly modify scroll_offset with no bounds.

New:
- Up: `scroll_offset += 1` (scroll up, away from bottom)
- Down: `scroll_offset = scroll_offset.saturating_sub(1)` (scroll toward bottom)
- On new message (TextDelta/ToolCallStart): reset `scroll_offset = 0` (auto-scroll to bottom)
- PageUp/PageDown: scroll by half screen height (stored as field or passed as param)

**Scroll indicator:** Show `"↑ more"` at top of message area when scrolled down (content above), show `"↓ new"` at bottom when scrolled up (content below).

### 2. Remove Permission Duplication

**File:** `src/tui/run.rs` — permission handler (line ~290)

Remove the `push_message` call for permission asks:

```rust
// REMOVE this block:
app.push_message(DisplayMessage {
    role: DisplayRole::Tool,
    content: format!("{}: {}", ask.tool_name, ask.description),
    tool_name: Some(ask.tool_name.clone()),
});
```

Permission info is already displayed in the input area via `InputMode::Permission`. The description is shown in the input block title or body. No duplication needed.

Similarly for Question asks — evaluate if `push_message` is needed or if the input area is sufficient.

### 3. Command Overlay (Drawer → Dropdown)

**File:** `src/tui/run.rs` — render loop

Current drawer rendering (lines 84-108):
```rust
if has_active {
    let chunks = Layout::horizontal([...]).split(area);
    ui::render_in_area(frame, &app, chunks[0]);
    tui_cmd.render(frame, chunks[1], active.state);
}
```

New — always render full-width chat, overlay the command on top:
```rust
// Always render full chat UI
ui::render(frame, &app);

// If a command is active, render it as overlay above input
if let Some(active) = &mut app.active_command {
    if let Some(Command::Tui(tui_cmd)) = app.command_registry.find(&active.command_name) {
        let overlay_area = compute_overlay_area(frame.area());
        frame.render_widget(Clear, overlay_area);
        tui_cmd.render(frame, overlay_area, active.state.as_mut());
    }
}
```

**`compute_overlay_area()`:** Position overlay above input area, similar to typeahead:
- Width: full width (or capped at 80 cols)
- Height: min(items + 2 border, terminal_height / 2)
- Y: input_area.y - height (above input, below messages)

**File:** `src/tui/ui.rs`

Add `pub fn compute_command_overlay_area(terminal_area: Rect) -> Rect` that calculates the overlay position. The input area starts at `terminal_area.height - 4` (3 for input + 1 for status bar), so the overlay bottom edge aligns there.

### 4. Key handling changes

When command overlay is active:
- Up/Down/Enter/Esc: forwarded to TUI command (existing behavior)
- Regular typing: forwarded to TUI command (existing behavior)
- **No change to key handling logic**, only rendering changes

## File Changes

| File | Changes |
|---|---|
| `src/tui/ui.rs` | Fix scroll in `render_messages()`, add `compute_command_overlay_area()`, add scroll indicators |
| `src/tui/app.rs` | Fix Up/Down scroll direction, add auto-scroll reset on new messages |
| `src/tui/run.rs` | Remove permission `push_message`, replace drawer layout with overlay rendering |

## Success Criteria

- [ ] Long messages are scrollable with Up/Down keys
- [ ] PageUp/PageDown scrolls by half page
- [ ] New messages auto-scroll to bottom
- [ ] Permission prompts appear ONLY in input area (no chat message)
- [ ] `/model` and other TUI commands render as overlay above input, not side drawer
- [ ] Existing tests pass
- [ ] 0 compiler warnings
