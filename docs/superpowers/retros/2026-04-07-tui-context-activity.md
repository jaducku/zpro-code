# Retrospective: TUI Polish + Context Window + Live Activity

**Date:** 2026-04-07
**Commits:** 48 on `dev` branch
**Tests:** 595 → 651

---

## What Was Done

### TUI Polish (11 tasks)
Markdown rendering (pulldown-cmark + syntect highlighting), tables with CJK-aware width, input history, multi-line input (Alt+Enter), enhanced status bar, tool result grouping, render cache.

### Context Window Management (7 tasks)
4-stage compaction pipeline: Snip (60%) → Microcompact (75%) → Full Compact/LLM (85%) → Hard Block (95%). Reactive compact on API overflow. Turn-aware snip preserves tool_use/tool_result pairs.

### Live Activity Display
Aggregated tool activity summary ("Searching for 2 patterns, reading 3 files…"), streaming status with elapsed time and tokens, braille spinner animation.

### Agent Display & UX Fixes
Original Claude Code style agent display with stats. Subagent event isolation. Ghost text fix. Stuck spinner fix. Non-blocking input with queued submission.

---

## Hard Lessons

### 1. `ResponseComplete` ≠ query done
`ResponseComplete` fires after EACH LLM streaming chunk. Setting `is_loading=false` there caused blank UI between tool completion and next LLM call. Fix: only `QueryCompleted` clears `is_loading`. Cost 3+ rounds of user debugging.

### 2. CJK display width breaks alignment
`chars().count()` ≠ display columns. Korean = 2 cols. Tables and code block right borders all broke. Abandoned right border entirely; added `display_width()` helper for tables.

### 3. Subagent events leak to parent TUI
Sharing `parent_ctx.event_tx` with child queries → child ToolCallStart appears as orphaned spinners in parent. Fix: separate `(child_event_tx, child_event_rx)` per subagent, collect stats from child channel.

### 4. Activity state must reset on ALL exit paths
`QueryCompleted`, `QueryCancelled`, `Error` — missing any one leaves the UI stuck. Every `is_loading = false` path needs matching `activity = default()` + timing resets.

### 5. ratatui diff rendering leaves ghost pixels
Shorter lines don't clear rightmost cells from previous frame. Fix: `Clear` widget before every `Paragraph` render. Scroll indicator area needs unconditional clear too.

### 6. `ToolResultContent` is an enum, not String
`Text(String) | Blocks(Vec<ContentBlock>)` — microcompact must handle both variants. Caught by spec reviewer, would have been a runtime bug.

---

## Architecture Decisions

| Decision | Choice | Why |
|----------|--------|-----|
| Markdown cache location | Local var in render loop, not in App | Avoids `&mut App` cascade through all render functions |
| Tool grouping | Render-time scan only | No structural change to DisplayMessage |
| Activity tracking | Count-based (counters + id→category map) | Simpler than per-tool entry tracking |
| Mouse mode | DEC 1000+1006 (not 1003) | 1003 blocks native text selection; Shift+drag is terminal standard |
| Input during loading | Queued submission on QueryCompleted | Less disruptive than cancel-and-restart |
| Code block border | Left-only (no right border) | Right border impossible to align with mixed CJK/ASCII |

---

## What Went Wrong

1. **Too many render bug iterations** — ghost text, stuck spinners, blank UI after tools. Should have audited the full event lifecycle upfront instead of fixing symptoms one at a time.
2. **ResponseComplete semantics misunderstood** — assumed it meant "query done" but it means "one streaming chunk done." This single misunderstanding caused the biggest user-facing bug.
3. **Spec review caught real issues but plan review missed others** — `ToolResultContent` type was caught, but `ResponseComplete` lifecycle wasn't questioned by any reviewer.
4. **CJK width was an afterthought** — should have been designed in from the start, not bolted on after the user saw broken rendering.

## What Went Right

1. **Subagent-driven development worked well** for the planned tasks (11 TUI + 7 context window). Each task isolated, tested, committed independently.
2. **User testing loop was fast** — screenshots → immediate fix → rebuild cycle caught UX issues that no automated test would find.
3. **Incremental delivery** — markdown, highlighting, tables, input history all landed as working increments before the bugfix rounds started.
