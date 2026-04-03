# atom

Open LLM-powered coding assistant CLI, written in Rust.

Multi-provider support (OpenRouter, Codex, Ollama, any OpenAI-compatible endpoint), interactive TUI with streaming, tool execution, permission system, and extensible skill/plugin architecture.

## Architecture

```
atom
├── src/
│   ├── main.rs                     # CLI entry (clap), provider setup, onboarding
│   ├── lib.rs                      # Library root
│   │
│   ├── entrypoints/
│   │   ├── print.rs                # --print non-interactive mode
│   │   └── setup.rs                # First-run onboarding wizard
│   │
│   ├── tui/                        # Terminal UI (ratatui + crossterm)
│   │   ├── app.rs                  # App state, input handling, typeahead
│   │   ├── run.rs                  # Event loop (EventStream + tokio::select!)
│   │   ├── ui.rs                   # Rendering (messages, status bar, dialogs)
│   │   └── event.rs                # Event types
│   │
│   ├── query/                      # LLM conversation engine
│   │   ├── engine.rs               # Query loop: LLM → tools → permission → repeat
│   │   └── types.rs                # QueryContext, QueryEvent, PermissionAsk
│   │
│   ├── commands/                   # ~110 slash commands
│   │   ├── types.rs                # CommandMeta, enums
│   │   ├── traits.rs               # PromptCommand, LocalCommand, TuiCommand
│   │   ├── registry.rs             # CommandRegistry (builtin + dynamic)
│   │   ├── prompt/                 # LLM prompt commands (/review, /commit, ...)
│   │   ├── local/                  # Text-returning commands (/clear, /exit, ...)
│   │   └── tui_cmds/              # Interactive TUI commands (/help, /config, /model, ...)
│   │
│   ├── services/
│   │   ├── api/                    # LLM Provider abstraction
│   │   │   ├── provider.rs         # LlmProvider trait
│   │   │   ├── openai_provider.rs  # OpenAI-compatible (SSE streaming + fallbacks)
│   │   │   ├── provider_factory.rs # Multi-provider factory
│   │   │   └── codex_auth.rs       # Codex CLI OAuth token reader
│   │   ├── config/                 # ~/.atom/config.json management
│   │   ├── context/                # AGENTS.md + git context assembly
│   │   ├── compact/                # Auto-compaction (token budget)
│   │   ├── hooks/                  # PreToolUse/PostToolUse shell hooks
│   │   ├── keychain/               # API key encryption
│   │   ├── mcp/                    # Model Context Protocol client
│   │   ├── session_transcript/     # Session save/restore
│   │   ├── skills/                 # SKILL.md dynamic loading
│   │   └── token_estimation/       # Token count estimation
│   │
│   ├── tools/                      # Built-in tools
│   │   ├── bash_tool.rs            # Shell command execution
│   │   ├── file_read_tool.rs       # File reading
│   │   ├── file_write_tool.rs      # File creation
│   │   ├── file_edit_tool.rs       # File editing
│   │   ├── glob_tool.rs            # File pattern matching
│   │   └── grep_tool.rs            # Content search
│   │
│   ├── utils/
│   │   ├── paths.rs                # Centralized path constants (~/.atom/)
│   │   └── permissions/            # Permission checker (5 modes + auto-classify)
│   │
│   └── types/                      # Shared types (messages, content, permissions)
│
├── docs/
│   ├── retrospective/              # Session retrospectives
│   └── superpowers/
│       ├── specs/                   # Design specifications
│       └── plans/                   # Implementation plans
│
└── Cargo.toml
```

## Features

### LLM Providers
- **OpenRouter** — Multi-model gateway with free models
- **Codex** — OpenAI Codex CLI OAuth token reuse
- **Ollama** — Local models
- **Custom** — Any OpenAI-compatible endpoint

Auto-fallback: system role unsupported → user message injection, tool use unsupported → retry without tools.

### Interactive TUI
- Streaming response display
- `/` slash command typeahead with fuzzy search
- `@` file mention with project file picker
- Right drawer for TUI commands (/config, /model, /mcp, ...)
- Permission dialog (Allow / Always Allow / Deny)
- CJK/Korean input support

### Tools
6 built-in tools + MCP server tools:
- **Bash** — Shell command execution
- **Read** — File reading
- **Write** — File creation
- **Edit** — File editing (string replacement)
- **Glob** — File pattern search
- **Grep** — Content search

### Permission System
5 modes matching the original CLI:
| Mode | Behavior |
|------|----------|
| **Auto** (default) | Read/Grep auto-allow, Bash/Write ask |
| **Default** | Ask for every tool call |
| **AcceptEdits** | Auto-allow edits, ask for Bash |
| **Plan** | Block all tool execution |
| **Bypass** | Allow everything |

### Context & Memory
- **AGENTS.md** — Open-standard project instructions (replaces CLAUDE.md)
- **Git context** — Branch, recent commits, working directory
- **Session persistence** — Auto-save, `--continue`, `--resume`
- **SKILL.md** — Dynamic skill loading from `.agents/skills/`

### Extensibility
- **MCP servers** — `.mcp.json` config, `atom mcp add-json/list/remove`
- **Shell hooks** — `.agents/hooks.json` (PreToolUse, PostToolUse)
- **Skills** — `.agents/skills/my-skill/SKILL.md` with YAML frontmatter

## Install

```bash
cargo install --path .
```

Binary installs to `~/.cargo/bin/atom`. Config lives at `~/.atom/`.

## Quick Start

```bash
# First run — interactive setup wizard
atom

# Or configure manually
atom config set provider openrouter
atom config set api_key sk-or-v1-...
atom config set default_model fast
atom config set fast google/gemma-3-27b-it:free
atom config set smart anthropic/claude-sonnet-4-20250514
```

## Usage

### Interactive Mode
```bash
atom                              # Start TUI with configured provider
atom --model smart                # Override model
atom --permission-mode default    # Ask before every tool
atom --dangerously-skip-permissions  # Skip all permission checks
atom --continue                   # Resume last session
atom --resume SESSION_ID          # Resume specific session
```

### Non-Interactive Mode
```bash
atom -p "explain this code"
atom -p "refactor main.rs" --output-format json
echo "list all TODO comments" | atom -p
atom -p "fix the bug" --max-turns 5 --max-budget-usd 0.50
```

### Slash Commands (in TUI)
```
/help          Show available commands
/model         Switch model (hot-swap)
/config        View settings
/mcp           Manage MCP servers
/memory        View AGENTS.md
/skills        List loaded skills
/session       View saved sessions
/resume        Resume a session
/diff          Show git diff
/branch        List git branches
/status        Show system status
/doctor        Run diagnostics
/clear         Clear conversation
/exit          Quit
```

### CLI Subcommands
```bash
atom config show|set|get|path     # Configuration
atom mcp list|add-json|get|remove # MCP server management
atom doctor                       # Health check
atom --help                       # Full CLI reference
```

## Configuration

### ~/.atom/config.json
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

### Project-level Config
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

### Global Config
```
~/.atom/
├── config.json               # Provider, models, settings
├── sessions/                 # Saved sessions
├── skills/                   # Global skills
├── hooks.json                # Global hooks
├── .mcp.json                 # Global MCP servers
└── AGENTS.md                 # Global instructions
```

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust |
| TUI | ratatui + crossterm |
| Async | tokio |
| HTTP | reqwest (streaming) |
| CLI | clap (derive) |
| Serialization | serde + serde_json + serde_yaml |
| MCP | Custom JSON-RPC + stdio transport |

## License

MIT
