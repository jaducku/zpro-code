# Agent 시스템: TypeScript vs Rust 마이그레이션 비교

## 1. Fork Subagent 구현

### TypeScript (`_reference_ts/src/tools/AgentTool/forkSubagent.ts`)

**특징:**
- **Feature gate**: `feature('FORK_SUBAGENT')` 번들 플래그로 활성화/비활성화
- **환경 조건**: 비대화형 세션, Coordinator 모드에서는 비활성화
- **Synthetic Agent**: `FORK_AGENT` 상수로 정의 (agentType='fork', maxTurns=200, model='inherit')
- **재귀 방지**: 메시지 스캔 (`isInForkChild()`) + boilerplate 태그 감지
- **메시지 구성**: 
  - Assistant 메시지 복제 (전체 tool_use, thinking, text)
  - 각 tool_use에 대해 `FORK_PLACEHOLDER_RESULT` (동일 텍스트, prompt cache 최적화)
  - Directive 텍스트 블록
- **Prompt cache**: 모든 fork child가 바이트 동일 API 요청 prefix 생성 → cache hit 최대화

```typescript
buildForkedMessages(directive: string, assistantMessage: AssistantMessage)
  → [fullAssistantMessage, toolResultMessage]
  // fullAssistantMessage: 원본 그대로 (모든 block 유지)
  // toolResultMessage: [...toolResultBlocks, {text: directive}]
```

---

### Rust (`src/services/fork_subagent.rs`)

**특징:**
- **Feature gate 없음**: `AppState.settings.fork_subagent` 설정값으로 활성화
- **환경 조건**: TS와 동일 (비대화형 세션, Coordinator 모드)
- **Synthetic Agent**: `FORK_AGENT` 상수 없음 (정의 필요 시 agentRuntime에서 구성)
- **재귀 방지**: 
  - Primary: `query_source == "agent:builtin:fork"` (autocompact 생존)
  - Fallback: `is_in_fork_child()` (메시지 스캔)
- **메시지 구성**: TS와 동일
  - `filter_incomplete_tool_calls()`: orphan tool_use 제거
  - Fork messages 확장

```rust
build_forked_messages(directive: &str, assistant_message: &ApiMessage)
  → Vec<ApiMessage>
  // [cloned_assistant, user_message]
```

**차이점:**
| 항목 | TypeScript | Rust |
|------|-----------|------|
| Feature gate | `feature()` 번들 플래그 | Config 설정값 |
| Recursion guard 1순위 | Message scanning | query_source ID (더 안정적) |
| Recursion guard 2순위 | (없음) | Message scanning |
| Promise.all 활용 | `Promise.all([...])` | Tokio 태스크 |
| Worktree notice 주입 | runAgent에서 | agent_runtime에서 |

---

## 2. Agent 실행 (runAgent)

### TypeScript (`_reference_ts/src/tools/AgentTool/runAgent.ts`)

**구조:**
```typescript
async function* runAgent({
  agentDefinition: AgentDefinition,
  promptMessages: Message[],
  toolUseContext: ToolUseContext,
  canUseTool: CanUseToolFn,
  isAsync: boolean,
  ...30+ options
}): AsyncGenerator<Message, void>
```

**특징:**
- **Async generator**: 메시지를 하나씩 yield (스트리밍 가능)
- **Context 분기**:
  - `forkContextMessages`: fork child는 parent 메시지 + 자신의 prompt
  - `promptMessages`: 일반 agent는 user prompt만
  - `filterIncompleteToolCalls()`: parent history에서 orphan 제거
- **파일 상태 캐싱**:
  - Fork child: `cloneFileStateCache(parent.readFileState)` (복제)
  - 일반 agent: `createFileStateCacheWithSizeLimit()` (신규)
- **Permission 모드**:
  - Agent 정의에서 override 가능 (parent 모드 우선순위 있음)
  - `agentPermissionMode` + `canShowPermissionPrompts` 조합
  - `allowedTools`: 세션 레벨 권한 제한
- **최적화**:
  - `omitClaudeMd`: Explore/Plan 등 읽기전용 agent는 CLAUDE.md 제외 (5-15 Gtok/week 절감)
  - `omitGitStatus`: Explore/Plan은 git status 제외 (1-3 Gtok/week 절감)
- **MCP 서버**:
  - `initializeAgentMcpServers()`: agent 정의의 mcpServers 연결
  - 부모 clients + agent 전용 clients 병합
  - Inline vs. named reference 처리
  - Cleanup 추적 (inline만 정리)
- **추적/메타**:
  - Perfetto tracing (hierarchy 시각화)
  - Transcript 저장 (subagents/ 그룹화)
  - onCacheSafeParams 콜백 (배경 요약용)

---

### Rust (`src/services/agent_runtime.rs`)

**구조:**
```rust
pub async fn run_foreground(
  params: AgentSpawnParams,
  parent_ctx: &ToolUseContext,
) -> Result<AgentResult>

pub async fn run_background(
  params: AgentSpawnParams,
  parent_ctx: &ToolUseContext,
) -> Result<ToolResult>
```

**특징:**
- **동기 실행**: `AgentResult` 반환 (스트리밍 없음, 완료 후 결과)
- **Context 생성**:
  - `build_child_context()`: 새로운 QueryContext (system_prompt + hooks 상속)
  - `build_fork_context()`: fork 메시지 + full parent history (max_turns=200)
  - Isolation worktree 지원 (`isolation: "worktree"`)
- **Background 실행**:
  - `tokio::spawn()` + `BackgroundTask` 등록
  - `QueryEvent::AgentCompleted` 전송
  - `pending_agent_results` 적재 (다음 user query와 병합)
- **Tool 제한**:
  - Foreground: 모든 builtin 도구 (Agent, Write, Edit 포함)
  - Background: 읽기만 (`BACKGROUND_ALLOWED_TOOLS`)
  - Fork: foreground와 동일 (full access)
- **Depth 제한**: `agent_depth >= 3` 초과 시 에러
- **Event relay**:
  - `relay_child_events()`: 자식 QueryEvent → parent TUI (실시간 전파)
  - `parent_agent_id` 설정 (TUI에서 중첩 표시)

**차이점:**
| 항목 | TypeScript | Rust |
|------|-----------|------|
| 반환 모델 | AsyncGenerator (스트리밍) | AgentResult (완료 후) |
| Foreground/Background | 분기 불명확 (`isAsync` 플래그) | 명시적 함수 분리 |
| MCP 추가 | Agent 내부 (initializeAgentMcpServers) | Parent context에서 상속 |
| Worktree 생성 | (없음) | `WorktreeService::create()` |
| Tool 제한 | allowedTools list (세션) | BACKGROUND_ALLOWED_TOOLS 상수 |
| Permission bubble | 메시지 채널 + UI | permission_ctx 복제 + TUI |

---

## 3. Provider Registry & Model Routing

### TypeScript (`_reference_ts/cli/src/utils/model/`)

**파일들:**
- `model.ts`: 스텁 (원본 구현 불명확)
- `modelStrings.ts`: Display 포맷팅
- `modelOptions.ts`: Model별 옵션
- `providers.ts`: 스텁

**구조 추론 (원본 코드 불명확):**
- `getMainLoopModel()`: 활성 모델 해석
- `parseUserSpecifiedModel()`: 사용자 입력 파싱
- Model alias 시스템 (명시적 registry 불명확)

---

### Rust (`src/services/api/registry.rs`)

**명시적 구조:**

```rust
pub struct ProviderRegistry {
  provider_templates: HashMap<String, ProviderTemplate>,
  // "or" → { api_base: "https://...", api_key: "..." }
  
  llm_configs: HashMap<String, ResolvedLlm>,
  // "sonnet" → { api_base: "...", model_id: "...", ... }
  
  legacy_models: HashMap<String, String>,
  // 구형 alias 호환성
  
  cache: RwLock<HashMap<String, Arc<dyn LlmProvider>>>,
  // 인스턴스 캐싱 (fast path)
}
```

**해석 순서 (resolve 메서드):**

1. **이름 기반**: `llm_configs.get(spec)`
   - 예: `"sonnet"` → `anthropic/claude-sonnet-4-20250514`
   - API cache hit (빠름)

2. **Ad-hoc provider:model**: `spec.split(':')` + `provider_templates.get(left)`
   - 예: `"or:gpt-4"` → OpenRouter 인스턴스 (즉석 생성)
   - 알려진 provider 사용

3. **Fallback _cli**: `provider_templates.get("_cli")`
   - Fallback provider (예: OpenAI CLI)
   - 미등록 모델도 처리

4. **Legacy alias**: `legacy_models.get(spec)`
   - 구형 config 호환성

**캐싱 전략:**
```rust
fn resolve_cached(cache_key: &str, llm: &ResolvedLlm) {
  // Fast path: read lock (99%)
  if cache.read().get(cache_key) { return }
  
  // Slow path: double-check + create + insert
  let mut cache = cache.write().unwrap();
  if !cache.contains_key(cache_key) {
    cache.insert(create_provider(llm));
  }
}
```

**Config 형식:**
```json
{
  "providers": {
    "or": {
      "api_base": "https://openrouter.ai/api/v1",
      "api_key": "sk-...",
      "model_options": {
        "gpt-4": { "reasoning": true, "reasoning_effort": "high" }
      }
    }
  },
  "models": {
    "sonnet": "or:anthropic/claude-sonnet",
    "qwen": "local:Qwen/Qwen2.5-72B"
  },
  "default_model": "sonnet"
}
```

**차이점 (TS는 명시되지 않아 추론):**
| 항목 | TypeScript | Rust |
|------|-----------|------|
| Registry 구조 | 불명확 (스텁) | 명시적 ProviderRegistry |
| 해석 순서 | 불명확 | 4단계 명확 (named → ad-hoc → _cli → legacy) |
| 캐싱 | 불명확 | RwLock + double-check |
| Ad-hoc provider:model | (불명확) | 지원 (즉석 생성) |
| Model info override | (불명확) | `context_window`, `max_output_tokens` 지원 |
| Reasoning config | (불명확) | `provider_options.reasoning` + `reasoning_effort` |

---

## 4. Agent Tool (AgentTool)

### TypeScript (`_reference_ts/src/tools/AgentTool/`)

**Files:**
- `runAgent.ts`: 메인 실행 로직 (async generator)
- `forkSubagent.ts`: Fork 메시지 구성
- `agentMemory.ts`: Session memory
- `agentColorManager.ts`: TUI 색상
- `agentToolUtils.ts`: 유틸
- `prompt.ts`: System prompt

**특징:**
- **Async generator yielding**: 실시간 메시지 스트리밍
- **Context 상속**: Parent의 모든 상태 (readFileState, toolUseContext, etc.)
- **Nested async support**: 부모가 async agent여도 자식 async agent 가능
- **Transcript 저장**: `recordSidechainTranscript()` + `writeAgentMetadata()`
- **메모리**: Session memory + 스냅샷

---

### Rust (`src/tools/agent_tool.rs`)

**구조:**
```rust
pub struct AgentTool;

impl Tool for AgentTool {
  async fn execute(&self, ctx: ToolUseContext) -> Result<ToolResult>
}
```

**특징:**
- **Foreground/Background split**: 
  - Foreground: `run_foreground()` → blocking, 완료 후 반환
  - Background: `run_background()` → async task, 즉시 반환
- **Fork detection**: `is_fork_enabled()` + `is_inside_fork()`
- **Depth 제한**: `agent_depth >= 3` 체크
- **Event 전송**: `AgentCompleted` 이벤트 (TUI 통지)

**단순화:**
- Foreground 스트리밍 없음 (전체 결과 후 반환)
- Transcript 저장 없음 (앞으로 구현 예정?)
- Memory 없음 (앞으로 구현 예정?)

---

## 5. Builtin Agent 시스템

### TypeScript

**정의:**
```typescript
export const FORK_AGENT = {
  agentType: 'fork',
  tools: ['*'],
  maxTurns: 200,
  model: 'inherit',
  permissionMode: 'bubble',
  getSystemPrompt: () => '',
} satisfies BuiltInAgentDefinition
```

**다른 builtin agents:**
- Explore (읽기 전용, omitClaudeMd)
- Plan (읽기 전용, omitGitStatus)
- 등등...

**로드 시스템:**
- `.agents/` 디렉토리에서 YAML/MD로 정의
- `loadAgentsDir.js`: 동적 로드
- `agentDisplay.js`: 렌더링

---

### Rust

**정의:**
- Fork subagent만 "builtin" 개념 (다른 agents는 plan, explore가 없음)
- `src/services/fork_subagent.rs`에서 하드코딩 (FORK_QUERY_SOURCE, FORK_CHILD_BODY)

**향후 확장:**
- Explore, Plan, Code Review agent 등 추가 필요 (Phase 2+)
- `.agents/` 디렉토리 스캔 및 로드 구현 필요

---

## 6. Key Architectural Differences

| 측면 | TypeScript | Rust |
|------|-----------|------|
| **Streaming** | AsyncGenerator (실시간 메시지) | 완료 후 결과 반환 |
| **Recursion Guard** | Message scanning | query_source ID (1순위) + message scanning (fallback) |
| **Permission Mode** | 메시지 채널 + UI | permission_ctx 복제 + abort signal |
| **Worktree Support** | (없음) | WorktreeService 완전 구현 |
| **Tool Filtering** | allowedTools list | BACKGROUND_ALLOWED_TOOLS 상수 |
| **MCP Integration** | Agent 내부 (initializeAgentMcpServers) | Parent context 상속 |
| **Transcript** | sidechain + metadata | (미구현) |
| **Model Registry** | 불명확 | 명시적 ProviderRegistry (4단계 해석) |
| **Builtin Agents** | 동적 로드 (.agents/) | 하드코딩 (fork만) |
| **Feature Gates** | Bun 번들 플래그 | Config 설정값 + 환경 조건 |

---

## 7. Migration Completeness

### ✅ 구현됨 (마이그레이션 완료)

- Fork subagent (메시지 구성, recursion guard)
- Agent spawn (foreground/background)
- Tool filtering (background restricted)
- Depth limiting (max 3)
- Worktree support (생성, cleanup)
- Permission context 상속
- MCP tool 공유
- Provider registry (model routing)
- Query source 추적

### ⏳ 구현 중 / 계획

- Streaming (async generator → async iterator로 전환)
- Transcript 저장 (sidechain, metadata)
- Session memory (agent memory snapshots)
- Builtin agents (Explore, Plan, Code Review)
- Feature gate (bun bundle → config)
- Event relay 최적화

### ❌ 미구현

- Agent color manager (TUI 색상)
- Agent memory snapshots
- Content replacement state (prompt cache stability)
- onCacheSafeParams 콜백 (배경 요약)
- Perfetto tracing (hierarchy visualization)
- Effort level override (agent definition)

---

## 8. Performance Optimizations

### TypeScript
- Prompt cache via byte-identical fork messages
- CLAUDE.md omit (Explore/Plan: 5-15 Gtok/week)
- Git status omit (Explore/Plan: 1-3 Gtok/week)
- File state cache clone (fork child)
- onCacheSafeParams (async background summary)

### Rust
- Provider caching (RwLock double-check)
- MCP tool sharing (Arc cloning, no duplication)
- Query event relay (real-time TUI update)
- Worktree auto-cleanup (no dangling branches)

---

## 9. Next Steps for Full Parity

1. **Streaming Support**: AsyncGenerator → async Iterator/Stream trait
2. **Transcript Management**: sidechain 저장 + metadata
3. **Builtin Agents**: Explore, Plan, Code Review, Debugging agent
4. **Memory System**: Agent memory snapshots + session memory
5. **Feature Gates**: Config-driven gates (build flags 제거)
6. **Advanced Optimizations**: Content replacement, effort level, analytics
7. **TUI Integration**: Agent color manager, event formatting
