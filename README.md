# claude-code-working

A working reverse-engineered Claude Code CLI codebase rebuilt from source analysis to reproduce the original terminal workflow, architecture, and engineering behavior.

[Start with the architecture docs](./docs/introduction/what-is-claude-code.mdx)

Current focus:

- [x] The codebase runs and passes baseline type checks.
- [x] The monorepo and engineering scaffolding are in place.
  - [ ] Biome formatting is intentionally conservative to avoid massive diff churn.
  - [x] The build pipeline produces runnable Node and Bun output.
- [x] The documentation set is available in English.
- [ ] More tests can still be added to improve confidence and stability.

## Quick start

### Environmental requirements

You must have the latest version of bun, otherwise there will be a bunch of weird bugs!!! bun upgrade!!!

- [Bun](https://bun.sh/) >= 1.3.11
- Conventional way to configure CC, each major provider has its own configuration method

### Installation

```bash
bun install
```

### Run

```bash
# Development mode, if you see the version number 888, it means it is correct
bun run dev

# Build
bun run build
```

The build uses code splitting multi-file packaging (`build.ts`), and the product is output to the `dist/` directory (entry `dist/cli.js` + about 450 chunk files).

The built version can be started by both bun and node. You can start it directly by publishing it to a private source.

If you encounter a bug, please raise an issue directly and we will resolve it first.

## Related documentation

- [Architecture overview](./docs/introduction/architecture-overview.mdx)
- [Conversation model](./docs/conversation/the-loop.mdx)
- [Tools and execution](./docs/tools/what-are-tools.mdx)
- [Safety and permissions](./docs/safety/why-safety-matters.mdx)
- [Extensibility](./docs/extensibility/mcp-protocol.mdx)

## Capability list

> ✅ = implemented ⚠️ = partially implemented / conditionally enabled ❌ = stub / removed / feature flag closed

### Core System

| Capabilities | Status | Description |
|------|------|------|
| REPL interactive interface (Ink terminal rendering) | ✅ | Home screen 5000+ lines, complete interaction |
| API Communication — Anthropic Direct | ✅ | Support API Key + OAuth |
| API Communication — AWS Bedrock | ✅ | Supports credential refresh, Bearer Token |
| API Communication — Google Vertex | ✅ | Support GCP credential refresh |
| API Communication — Azure Foundry | ✅ | Supports API Key + Azure AD |
| Streaming dialogue and tool call loop (`query.ts`) | ✅ | 1700+ lines, including automatic compression and token tracking |
| Conversation Engine (`QueryEngine.ts`) | ✅ | 1300+ lines to manage conversation state and attribution |
| Context build (git status / CLAUDE.md / memory) | ✅ | `context.ts` complete implementation |
| Permission system (plan/auto/manual mode) | ✅ | 6300+ lines, including YOLO classifier, path verification, rule matching |
| Hook system (pre/post tool use) | ✅ | Support settings.json configuration |
| Session Resume (`/resume`) | ✅ | Standalone ResumeConversation screen |
| Doctor Diagnosis (`/doctor`) | ✅ | Version, API, plugin, sandbox check |
| Automatic compression (compaction) | ✅ | auto-compact / micro-compact / API compact |

### Tools — always available

| Tools | Status | Description |
|------|------|------|
| BashTool | ✅ | Shell execution, sandbox, permission check |
| FileReadTool | ✅ | File / PDF / Picture / Notebook Reading |
| FileEditTool | ✅ | String replacement editing + diff tracking |
| FileWriteTool | ✅ | File creation/overwriting + diff generation |
| NotebookEditTool | ✅ | Jupyter Notebook cell editing |
| AgentTool | ✅ | Subagent fork (fork/async/background/remote) |
| WebFetchTool | ✅ | URL Fetching → Markdown → AI Summary |
| WebSearchTool | ✅ | Web search + domain name filtering |
| AskUserQuestionTool | ✅ | Multiple question interactive prompts + preview |
| SendMessageTool | ✅ | Message sending (peers / teammates / mailbox) |
| SkillTool | ✅ | Slash command / Skill call |
| EnterPlanModeTool | ✅ | Enter planning mode |
| ExitPlanModeTool (V2) | ✅ | Exit plan mode |
| TodoWriteTool | ✅ | Todo List v1 |
| BriefTool | ✅ | Short message + attachment sending |
| TaskOutputTool | ✅ | Background task output reading |
| TaskStopTool | ✅ | Background task stop |
| ListMcpResourcesTool | ⚠️ | MCP resource list (filtered by specialTools, added under specific conditions) |
| ReadMcpResourceTool | ⚠️ | MCP resource reading (same as above) |
| SyntheticOutputTool | ⚠️ | Created only in non-interactive sessions (SDK/pipe mode) |
| CronCreateTool | ✅ | Scheduled task creation (AGENT_TRIGGERS gate has been removed) |
| CronDeleteTool | ✅ | Scheduled task deletion |
| CronListTool | ✅ | Scheduled task list |
| EnterWorktreeTool | ✅ | Enter Git Worktree (`isWorktreeModeEnabled()` is hardcoded to true) |
| ExitWorktreeTool | ✅ | Exit Git Worktree |

### Tools — Conditional Enablement

| Tools | Status | Enablement Conditions |
|------|------|----------|
| GlobTool | ✅ | Enabled when bfs/ugrep is not embedded (enabled by default) |
| GrepTool | ✅ | Same as above |
| TaskCreateTool | ⚠️ | When `isTodoV2Enabled()` is true |
| TaskGetTool | ⚠️ | Same as above |
| TaskUpdateTool | ⚠️ | Same as above |
| TaskListTool | ⚠️ | Same as above |
| TeamCreateTool | ⚠️ | `isAgentSwarmsEnabled()` |
| TeamDeleteTool | ⚠️ | Same as above |
| ToolSearchTool | ⚠️ | `isToolSearchEnabledOptimistic()` |
| PowerShellTool | ⚠️ | Windows Platform Detection |
| LSPTool | ⚠️ | `ENABLE_LSP_TOOL` environment variable |
| ConfigTool | ❌ | `USER_TYPE === 'ant'` (always false) |

### Tools — Feature Flag Off (all disabled)

| Tools | Feature Flag |
|------|-------------|
| SleepTool | `PROACTIVE` / `KAIROS` |
| RemoteTriggerTool | `AGENT_TRIGGERS_REMOTE` |
| MonitorTool | `MONITOR_TOOL` |
| SendUserFileTool | `KAIROS` |
| OverflowTestTool | `OVERFLOW_TEST_TOOL` |
| TerminalCaptureTool | `TERMINAL_PANEL` |
| WebBrowserTool | `WEB_BROWSER_TOOL` |
| SnipTool | `HISTORY_SNIP` |
| WorkflowTool | `WORKFLOW_SCRIPTS` |
| PushNotificationTool | `KAIROS` / `KAIROS_PUSH_NOTIFICATION` |
| SubscribePRTool | `KAIROS_GITHUB_WEBHOOKS` |
| ListPeersTool | `UDS_INBOX` |
| CtxInspectTool | `CONTEXT_COLLAPSE` |

### Tools — Stub / Not available

| Tools | Instructions |
|------|------|
| TungstenTool | ANT-ONLY stub |
| REPLTool | ANT-ONLY，`isEnabled: () => false` |
| SuggestBackgroundPRTool | ANT-ONLY，`isEnabled: () => false` |
| VerifyPlanExecutionTool | Requires `CLAUDE_CODE_VERIFY_PLAN=true` environment variable and is stub |
| ReviewArtifactTool | stub, not registered to tools.ts |
| DiscoverSkillsTool | stub, not registered to tools.ts |

### Slash command — available

| Command | Status | Description |
|------|------|------|
| `/add-dir` | ✅ | Add directory |
| `/advisor` | ✅ | Advisor Configuration |
| `/agents` | ✅ | Agent list/management |
| `/branch` | ✅ | Branch management |
| `/btw` | ✅ | Quick Notes |
| `/chrome` | ✅ | Chrome integration |
| `/clear` | ✅ | Clear screen |
| `/color` | ✅ | Agent color |
| `/compact` | ✅ | Compact conversation |
| `/config` (`/settings`) | ✅ | Configuration management |
| `/context` | ✅ | Context information |
| `/copy` | ✅ | Copy last message |
| `/cost` | ✅ | Session cost |
| `/desktop` | ✅ | Claude Desktop Integration |
| `/diff` | ✅ | show diff |
| `/doctor` | ✅ | Health Check |
| `/effort` | ✅ | Set effort level |
| `/exit` | ✅ | Exit |
| `/export` | ✅ | Export conversation |
| `/extra-usage` | ✅ | Extra usage information |
| `/fast` | ✅ | Switch fast mode |
| `/feedback` | ✅ | Feedback |
| `/loop` | ✅ | Timed loop execution (bundled skill, can be turned off via `CLAUDE_CODE_DISABLE_CRON`) |
| `/heapdump` | ✅ | Heap dump (debug) |
| `/help` | ✅ | Help |
| `/hooks` | ✅ | Hook Management |
| `/ide` | ✅ | IDE connection |
| `/init` | ✅ | Initialization project |
| `/install-github-app` | ✅ | Install GitHub App |
| `/install-slack-app` | ✅ | Install Slack App |
| `/keybindings` | ✅ | Shortcut key management |
| `/login` / `/logout` | ✅ | Login / Logout |
| `/mcp` | ✅ | MCP Service Management |
| `/memory` | ✅ | Memory / CLAUDE.md Management |
| `/mobile` | ✅ | Mobile QR code |
| `/model` | ✅ | Model selection |
| `/output-style` | ✅ | Output style |
| `/passes` | ✅ | Recommendation code |
| `/permissions` | ✅ | Permission management |
| `/plan` | ✅ | Plan mode |
| `/plugin` | ✅ | Plug-in management |
| `/pr-comments` | ✅ | PR comments |
| `/privacy-settings` | ✅ | Privacy Settings |
| `/rate-limit-options` | ✅ | Rate limit options |
| `/release-notes` | ✅ | Update log |
| `/reload-plugins` | ✅ | Reload plugins |
| `/remote-env` | ✅ | Remote environment configuration |
| `/rename` | ✅ | Rename session |
| `/resume` | ✅ | Resume session |
| `/review` | ✅ | Code review (local) |
| `/ultrareview` | ✅ | Cloud review |
| `/rewind` | ✅ | Rewind conversation |
| `/sandbox-toggle` | ✅ | Toggle sandbox |
| `/security-review` | ✅ | Security review |
| `/session` | ✅ | Session information |
| `/skills` | ✅ | Skill Management |
| `/stats` | ✅ | Session statistics |
| `/status` | ✅ | Status information |
| `/statusline` | ✅ | Status bar UI |
| `/stickers` | ✅ | Stickers |
| `/tasks` | ✅ | Task management |
| `/theme` | ✅ | Terminal theme |
| `/think-back` | ✅ | Year in Review |
| `/upgrade` | ✅ | Upgrade CLI |
| `/usage` | ✅ | Usage information |
| `/insights` | ✅ | Usage analysis report |
| `/vim` | ✅ | Vim mode |

### Slash Command — Feature Flag Off

| Command | Feature Flag |
|------|-------------|
| `/voice` | `VOICE_MODE` |
| `/proactive` | `PROACTIVE` / `KAIROS` |
| `/brief` | `KAIROS` / `KAIROS_BRIEF` |
| `/assistant` | `KAIROS` |
| `/remote-control` (alias `rc`) | `BRIDGE_MODE` |
| `/remote-control-server` | `DAEMON` + `BRIDGE_MODE` |
| `/force-snip` | `HISTORY_SNIP` |
| `/workflows` | `WORKFLOW_SCRIPTS` |
| `/web-setup` | `CCR_REMOTE_SETUP` |
| `/subscribe-pr` | `KAIROS_GITHUB_WEBHOOKS` |
| `/ultraplan` | `ULTRAPLAN` |
| `/torch` | `TORCH` |
| `/peers` | `UDS_INBOX` |
| `/fork` | `FORK_SUBAGENT` |
| `/buddy` | `BUDDY` |

### Slash command — ANT-ONLY (not available)

`/files` `/tag` `/backfill-sessions` `/break-cache` `/bughunter` `/commit` `/commit-push-pr` `/ctx_viz` `/good-claude` `/issue` `/init-verifiers` `/mock-limits` `/bridge-kick` `/version` `/reset-limits` `/onboarding` `/share` `/summary` `/teleport` `/ant-trace` `/perf-issue` `/env` `/oauth-refresh` `/debug-tool-call` `/agents-platform` `/autofix-pr`

### CLI subcommands

| Subcommand | Status | Description |
|--------|------|------|
| `claude` (default) | ✅ | Main REPL / interactive / print mode |
| `claude mcp serve/add/remove/list/get/...` | ✅ | MCP service management (7 subcommands) |
| `claude auth login/status/logout` | ✅ | Authentication management |
| `claude plugin validate/list/install/...` | ✅ | Plug-in management (7 subcommands) |
| `claude setup-token` | ✅ | Long-term Token configuration |
| `claude agents` | ✅ | Agent list |
| `claude doctor` | ✅ | Health Check |
| `claude update` / `upgrade` | ✅ | Automatic update |
| `claude install [target]` | ✅ | Native installation |
| `claude server` | ❌ | `DIRECT_CONNECT` flag |
| `claude ssh <host>` | ❌ | `SSH_REMOTE` flag |
| `claude open <cc-url>` | ❌ | `DIRECT_CONNECT` flag |
| `claude auto-mode` | ❌ | `TRANSCRIPT_CLASSIFIER` flag |
| `claude remote-control` | ❌ | `BRIDGE_MODE` + `DAEMON` flag |
| `claude assistant` | ❌ | `KAIROS` flag |
| `claude up/rollback/log/error/export/task/completion` | ❌ | ANT-ONLY |

### Service layer

| Service | Status | Description |
|------|------|------|
| API Client (`services/api/`) | ✅ | 3400+ lines, 4 providers |
| MCP (`services/mcp/`) | ✅ | 34 files, 12000+ lines |
| OAuth (`services/oauth/`) | ✅ | Complete OAuth process |
| Plugins (`services/plugins/`) | ✅ | Complete infrastructure, no built-in plugins |
| LSP (`services/lsp/`) | ⚠️ | Implementation exists, closed by default |
| Compression (`services/compact/`) | ✅ | auto / micro / API compression |
| Hook system (`services/tools/toolHooks.ts`) | ✅ | pre/post tool use hooks |
| Session Memory (`services/SessionMemory/`) | ✅ | Session Memory Management |
| Memory extraction (`services/extractMemories/`) | ✅ | Automatic memory extraction |
| Skill Search (`services/skillSearch/`) | ✅ | Local/remote skill search |
| Policy Limits (`services/policyLimits/`) | ✅ | Policy Limit Enforcement |
| Analysis / GrowthBook / Sentry | ⚠️ | The frame exists, the actual sink is empty |
| Voice (`services/voice.ts`) | ❌ | `VOICE_MODE` flag off |

### Internal packages (`packages/`)

| Package | Status | Description |
|------|------|------|
| `color-diff-napi` | ✅ | 1006 lines of complete TypeScript implementation (syntax highlighting diff) |
| `audio-capture-napi` | ✅ | 151 lines of complete implementation (cross-platform audio recording, using SoX/arecord) |
| `image-processor-napi` | ✅ | 125 lines of complete implementation (macOS clipboard image reading, using osascript + sharp) |
| `modifiers-napi` | ✅ | 67 lines of complete implementation (macOS modifier key detection, bun:ffi + CoreGraphics) |
| `url-handler-napi` | ❌ | stub, `waitForUrlEvent()` returns null |
| `@ant/claude-for-chrome-mcp` | ❌ | stub, `createServer()` returns null |
| `@ant/computer-use-mcp` | ⚠️ | Type-safe stub (line 265, complete type definition but function returns null) |
| `@ant/computer-use-input` | ✅ | 183 lines of complete implementation (macOS keyboard and mouse simulation, AppleScript/JXA/CGEvent) |
| `@ant/computer-use-swift` | ✅ | 388 lines of complete implementation (macOS display/application management/screenshot, JXA/screencapture) |

### Feature Flags (31, all return `false`)

`ABLATION_BASELINE` `AGENT_MEMORY_SNAPSHOT` `BG_SESSIONS` `BRIDGE_MODE` `BUDDY` `CCR_MIRROR` `CCR_REMOTE_SETUP` `CHICAGO_MCP` `COORDINATOR_MODE` `DAEMON` `DIRECT_CONNECT` `EXPERIMENTAL_SKILL_SEARCH` `FORK_SUBAGENT` `HARD_FAIL` `HISTORY_SNIP` `KAIROS` `KAIROS_BRIEF` `KAIROS_CHANNELS` `KAIROS_GITHUB_WEBHOOKS` `LODESTONE` `MCP_SKILLS` `PROACTIVE` `SSH_REMOTE` `TORCH` `TRANSCRIPT_CLASSIFIER` `UDS_INBOX` `ULTRAPLAN` `UPLOAD_USER_SETTINGS` `VOICE_MODE` `WEB_BROWSER_TOOL` `WORKFLOW_SCRIPTS`

## Project structure

```
claude-code/
├── src/
│   ├── entrypoints/
│ │ ├── cli.tsx # Entry file (including MACRO/feature polyfill)
│ │ └── sdk/ # SDK submodule stub
│ ├── main.tsx # Main CLI logic (Commander definition)
│   └── types/
│ ├── global.d.ts # Global variable/macro declaration
│ └── internal-modules.d.ts # Internal npm package type declaration
├── packages/ # Monorepo workspace package
│ ├── color-diff-napi/ # Complete implementation (terminal color diff)
│ ├── modifiers-napi/ # stub (macOS modifier key detection)
│   ├── audio-capture-napi/  # stub
│   ├── image-processor-napi/# stub
│   ├── url-handler-napi/    # stub
│ └── @ant/ # Anthropic internal package stub
│       ├── claude-for-chrome-mcp/
│       ├── computer-use-mcp/
│       ├── computer-use-input/
│       └── computer-use-swift/
├── scripts/ # Automatic stub generation script
├── build.ts # Build script (Bun.build + code splitting + Node.js compatible post-processing)
├── dist/ # Build output (entry cli.js + ~450 chunk files)
└── package.json # Bun workspaces monorepo configuration
```

## Technical description

### Runtime Polyfill

The necessary polyfill is injected at the top of the entry file `src/entrypoints/cli.tsx`:

- `feature()` — return `false` for all feature flags, skip unimplemented branches
- `globalThis.MACRO` — emulates build-time macro injection (VERSION, etc.)

### Monorepo

The project uses Bun workspaces to manage internal packages. The stubs that were originally placed manually under `node_modules/` have been uniformly moved into `packages/` and parsed through `workspace:*`.

## Feature Flags detailed explanation

The original Claude Code injects feature flags during build through `feature()` of `bun:bundle`, and the grayscale release is controlled by A/B experiment platforms such as GrowthBook. In this project, `feature()` is polyfilled to always return `false`, so the following 30 flags are all turned off.

### Autonomous Agent

| Flag | Purpose |
|------|------|
| `KAIROS` | Assistant mode - long-running autonomous Agent (including brief, push notification, file sending) |
| `KAIROS_BRIEF` | Kairos Brief — Send briefing summary to user |
| `KAIROS_CHANNELS` | Kairos Channels — Multi-Channel Communication |
| `KAIROS_GITHUB_WEBHOOKS` | GitHub Webhook subscription — PR events are pushed to Agent in real time |
| `PROACTIVE` | Active mode - Agent actively performs tasks, including SleepTool scheduled wake-up |
| `COORDINATOR_MODE` | Coordinator mode - multi-Agent orchestration and scheduling |
| `BUDDY` | Buddy pairing programming function |
| `FORK_SUBAGENT` | Fork subagent — fork an independent subagent from the current session |

### Remote/Distributed

| Flag | Purpose |
|------|------|
| `BRIDGE_MODE` | Remote control bridge — allows external clients to remotely control Claude Code |
| `DAEMON` | Daemon process — background resident service, supports worker and supervisor |
| `BG_SESSIONS` | Background session — `ps`/`logs`/`attach`/`kill`/`--bg` and other background process management |
| `SSH_REMOTE` | SSH remote — `claude ssh <host>` connect to remote host |
| `DIRECT_CONNECT` | Direct connection mode - `cc://` URL protocol, server command, `open` command |
| `CCR_REMOTE_SETUP` | Web remote configuration - configure through browser Claude Code |
| `CCR_MIRROR` | Claude Code Runtime Mirroring - Session State Sync/Replication |

### Communication

| Flag | Purpose |
|------|------|
| `UDS_INBOX` | Unix Domain Socket Inbox - Local communication between Agents (`/peers`) |

### Enhancement Tools

| Flag | Purpose |
|------|------|
| `CHICAGO_MCP` | Computer Use MCP — Computer operation (screenshot, mouse and keyboard control) |
| `WEB_BROWSER_TOOL` | Web browser tool - embedded browser interaction in the terminal |
| `VOICE_MODE` | Voice mode - voice input and output, microphone push-to-talk |
| `WORKFLOW_SCRIPTS` | Workflow scripts — user-defined automated workflows |
| `MCP_SKILLS` | MCP-based Skill loading mechanism |

### Dialogue management

| Flag | Purpose |
|------|------|
| `HISTORY_SNIP` | History clipping — manually clip snippets in conversation history (`/force-snip`) |
| `ULTRAPLAN` | Ultra Plan — Large-scale planning capabilities for remote Agent collaboration |
| `AGENT_MEMORY_SNAPSHOT` | Memory snapshot function when Agent is running |

### Infrastructure/Experiments

| Flag | Purpose |
|------|------|
| `ABLATION_BASELINE` | Science Experiment — Baseline ablation test for A/B experiment control group |
| `HARD_FAIL` | Hard failure mode - interrupt directly instead of downgrading when an error occurs |
| `TRANSCRIPT_CLASSIFIER` | Conversation classifier — `auto-mode` command, automatically analyzes and classifies conversation records |
| `UPLOAD_USER_SETTINGS` | Setting synchronization upload — synchronize local configuration to the cloud |
| `LODESTONE` | Deep link protocol handler - jump from external application to the location specified by Claude Code |
| `EXPERIMENTAL_SKILL_SEARCH` | Experimental Skill Search Index |
| `TORCH` | Torch function (specific use unknown, may be some kind of highlighting/tracking mechanism) |

## License

This project is for study and research purposes only. All rights to Claude Code belong to [Anthropic](https://www.anthropic.com/).
