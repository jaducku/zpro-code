# Local Command Execution & Real Logic — Design Spec

## Overview

Fix the local command execution pipeline so `.call()` is actually invoked, and fill in real logic for commands that don't depend on Phase 9 services.

## 1. Fix dispatch pipeline (run.rs)

Currently `dispatch_slash_command()` doesn't call `.call()` on local commands — it just shows a "/{name}: {description} (executed)" message. Fix this to:

1. Actually call `cmd.call(args, &ctx)` asynchronously
2. Handle the `LocalCommandResult`:
   - `Text { value }` → show as system message
   - `Compact { display_text }` → clear messages + show display_text
   - `Skip` → no output
3. Special-case `exit` — after `.call()`, set `app.should_quit = true`
4. Remove the `is_clear` special case — let ClearCommand's `.call()` handle it via `MessageUpdate::Set` channel (or keep handling in run.rs since the channel isn't connected yet — pragmatic choice)

Since `dispatch_slash_command` is sync but `.call()` is async, the function needs to become async or use `tokio::runtime::Handle::current().block_on()`.

## 2. CommandContext with real state

Currently TUI/Local dispatch creates a dummy `AppState::default()`. Instead, share the real app state. Since `App` fields (messages, model, etc.) live directly on `App` — not inside `AppState` — the pragmatic approach is:

- For `cost`: read `app.token_count` and `app.session_cost` directly (not via CommandContext)
- For `exit`: return a special result, handle in run.rs
- For commands that need AppState: pass `Arc<RwLock<AppState>>` when available

## 3. Local commands to fill in

| Command | Logic |
|---------|-------|
| `exit` | Return text "Goodbye!", run.rs sets `should_quit = true` |
| `cost` | Read token_count/session_cost from FoundCommand data extraction |
| `version` | Return `env!("CARGO_PKG_VERSION")` (already has this) |
| `vim` | Return toggle message (actual vim mode is Phase 7 VIM module) |
| `copy` | Return "Copied!" placeholder (real clipboard needs platform APIs) |

## 4. TUI commands to fill in

| Command | Logic |
|---------|-------|
| `status` | Show version, model, connection status, token count, cost |
| `model` | Show current model name, instructions to change |
| `plan` | Toggle message for plan mode |

## 5. Not in scope

- LLM integration for prompt commands (needs query engine wiring)
- Service-dependent commands (compact, session, mcp, skills, etc.)
- CLI subcommand real handlers (Phase 9+)
