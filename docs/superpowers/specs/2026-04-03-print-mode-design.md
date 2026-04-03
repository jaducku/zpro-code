# --print Mode (Non-Interactive) — Design Spec

## Overview

Non-interactive mode: read prompt from CLI arg or stdin, call LLM, output result to stdout, exit. Supports text/json/stream-json output formats.

## Usage

```bash
atom -p "explain this code"                          # prompt as arg
echo "refactor main.rs" | atom -p                    # prompt from stdin
atom -p "list files" --output-format json             # JSON output
atom -p "list files" --output-format stream-json      # streaming JSON
```

## Implementation

New function `run_print_mode()` in `src/entrypoints/print.rs`:

1. Get prompt: `cli.prompt` or read all of stdin
2. Build system prompt via `assemble_system_prompt()`
3. Create QueryContext with bypass permissions (no TUI for dialogs)
4. Call `query()` directly (no TUI, no event loop)
5. Output based on `--output-format`:
   - `text`: final assistant text to stdout
   - `json`: `{"result":"...","input_tokens":N,"output_tokens":N}` to stdout
   - `stream-json`: each event as JSON line to stdout
6. Tool output → stderr
7. Exit with 0 (success) or 1 (error)

## Permission
Print mode defaults to bypass (no interactive dialog possible). Warn on stderr if tools are used.
