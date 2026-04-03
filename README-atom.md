# atom-code

Open LLM-powered coding assistant CLI, written in Rust.

## Install

```bash
cargo install --path .
```

## Usage

### Interactive (TUI)

```bash
# OpenRouter
atom --api-base https://openrouter.ai/api/v1 --api-key $KEY --model gpt-4o

# Codex OAuth
atom --provider codex

# Local model (Ollama)
atom --api-base http://localhost:11434/v1 --model llama3
```

### Non-Interactive (--print)

```bash
atom -p "explain this code" --api-base URL --api-key KEY --model MODEL
echo "refactor main.rs" | atom -p --output-format json ...
```

### Session Management

```bash
atom --continue          # Resume last session
atom --resume SESSION_ID # Resume specific session
```

### MCP Servers

```bash
atom mcp add-json context7 '{"command":"npx","args":["-y","@upstash/context7-mcp"]}'
atom mcp list
atom mcp remove context7
```

### Permission Modes

```bash
atom --permission-mode default       # Ask for each tool use
atom --permission-mode auto          # Auto-approve safe operations
atom --permission-mode plan          # Planning only, no tool execution
atom --dangerously-skip-permissions  # Bypass all checks
```

### Safety Limits

```bash
atom -p "prompt" --max-turns 10          # Limit agent loops
atom -p "prompt" --max-budget-usd 1.00   # Limit API spend
```

## Project Context

Place `AGENTS.md` in your project root for automatic context loading. Place skills in `.agents/skills/my-skill/SKILL.md`.

Global config: `~/.atom/`

## Features

- Multi-provider LLM (Codex OAuth, OpenRouter, Ollama, vLLM, any OpenAI-compatible)
- SSE streaming with system role fallback for incompatible models
- Tool execution (Bash, Read, Write, Edit, Glob, Grep) + MCP tools
- Permission system (5 modes) with TUI dialog
- AGENTS.md context + git branch/commits
- Session auto-save + resume
- PreToolUse/PostToolUse shell hooks
- Slash command typeahead + @ file mentions
- SKILL.md dynamic loading
- --print mode (text/json/stream-json)
- Token tracking + auto-compact
- CJK/Korean input support
