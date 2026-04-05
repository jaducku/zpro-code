# zpro-code

Open LLM-powered coding assistant CLI, written in Rust.

Multi-provider support (OpenRouter, Codex, Ollama, any OpenAI-compatible endpoint), interactive TUI with streaming, tool execution, permission system, MCP integration, and extensible skill/plugin architecture.

## Install

```bash
# One-line install (macOS / Linux)
curl -fsSL https://raw.githubusercontent.com/jaducku/zpro-code/main/install.sh | sh

# Or build from source
cargo install --path .
```

Binary: `zpro-code` | Config: `~/.zpro-code/`

## Quick Start

```bash
# First run — interactive onboarding wizard
zpro-code

# Or configure manually
zpro-code config set provider openrouter
zpro-code config set api_key sk-or-v1-...
zpro-code config set default_model fast
zpro-code config set fast google/gemma-3-27b-it:free
zpro-code config set smart anthropic/claude-sonnet-4-20250514
```

## Usage

### Interactive mode
```bash
zpro-code                                # TUI with configured provider
zpro-code --model smart                  # Override model
zpro-code --permission-mode default      # Ask for every tool call
zpro-code --continue                     # Resume last session
zpro-code --resume SESSION_ID            # Resume specific session
```

### Non-interactive mode
```bash
zpro-code -p "Explain this code"
zpro-code -p "Refactor main.rs" --output-format json
echo "Find all TODO comments" | zpro-code -p
zpro-code -p "Fix the bug" --max-turns 5 --max-budget-usd 0.50
```

### Slash commands (inside TUI)
```
/help       Show available commands
/model      Switch model (hot-swap mid-conversation)
/config     View configuration
/mcp        Manage MCP servers
/memory     View AGENTS.md contents
/skills     List loaded skills
/session    List saved sessions
/resume     Restore a session
/diff       View git diff
/branch     List git branches
/status     System status
/doctor     Run diagnostics
/clear      Reset conversation
/exit       Quit
```

### CLI subcommands
```bash
zpro-code config show|set|get|path       # Configuration management
zpro-code mcp list|add-json|get|remove   # MCP server management
zpro-code doctor                         # Health check
zpro-code --help                         # Full CLI reference
```

## LLM Providers

- **OpenRouter** — Multi-model gateway, free model support
- **Codex** — Reuse OpenAI Codex CLI OAuth tokens
- **Ollama** — Local models
- **Custom** — Any OpenAI-compatible endpoint

Auto-fallback: system role unsupported → inject as user message, tool use unsupported → retry without tools.

## Tools

Built-in tools + MCP server tools:

| Tool | Description |
|------|-------------|
| **Bash** | Shell command execution |
| **Read** | File reading |
| **Write** | File creation |
| **Edit** | File editing (string replacement) |
| **Glob** | File pattern matching |
| **Grep** | Code content search |
| **WebFetch** | Fetch and extract web content |
| **WebSearch** | Web search |
| **Agent** | Spawn sub-agents |
| **Task** | Task management (create/update/list) |
| **Notebook** | Jupyter notebook editing |
| **MCP** | MCP resource access |

## Permission System

| Mode | Behavior |
|------|----------|
| **Auto** (default) | Auto-allow Read/Grep, ask for Bash/Write |
| **Default** | Ask for every tool call |
| **AcceptEdits** | Auto-allow edits, ask for Bash |
| **Plan** | Block all tool execution (plan only) |
| **Bypass** | Allow everything |

## Context & Memory

- **AGENTS.md** — Open-standard project instructions (CLAUDE.md alternative)
- **Git context** — Branch, recent commits, working directory
- **Session save** — Auto-save, `--continue`, `--resume`
- **SKILL.md** — Dynamic skill loading from `.agents/skills/`

## Extensibility

- **MCP servers** — `.mcp.json` config, `zpro-code mcp add-json/list/remove`
- **Shell hooks** — `.agents/hooks.json` (PreToolUse, PostToolUse)
- **Skills** — `.agents/skills/my-skill/SKILL.md` (YAML frontmatter)

## Configuration

### ~/.zpro-code/config.json
```json
{
  "provider": "openrouter",
  "api_key": "enc:...",
  "default_model": "fast",
  "models": {
    "fast": "google/gemma-3-27b-it:free",
    "smart": "anthropic/claude-sonnet-4-20250514",
    "code": "deepseek/deepseek-chat-v3-0324:free"
  },
  "permission_mode": "auto"
}
```

API keys are encrypted in the config file (machine-specific XOR + base64).

### Project-level config
```
your-project/
├── AGENTS.md                 # Project instructions (auto-loaded)
├── .mcp.json                 # Project MCP servers
└── .agents/
    ├── skills/               # Project skills
    │   └── my-skill/
    │       └── SKILL.md
    └── hooks.json            # Project hooks
```

## Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust |
| TUI | ratatui + crossterm |
| Async | tokio |
| HTTP | reqwest (rustls) |
| CLI | clap (derive) |
| Serialization | serde + serde_json + serde_yaml |
| MCP | JSON-RPC + stdio transport |

## License

MIT
