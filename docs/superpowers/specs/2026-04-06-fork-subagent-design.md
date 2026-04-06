# Fork Subagent Design Spec

## Overview

Fork subagent는 부모의 전체 대화 컨텍스트와 system prompt를 상속하는 서브에이전트다. 일반 agent가 새 대화를 시작하는 반면, fork는 부모 대화를 이어받아 실행하며, 바이트 동일한 prefix로 prompt cache hit를 극대화한다.

**TypeScript 원본 참조:**
- `src/tools/AgentTool/forkSubagent.ts` — 메시지 구성, 재귀 guard, 상수
- `src/tools/AgentTool/AgentTool.tsx` — fork 분기 결정, system prompt 준비
- `src/tools/AgentTool/runAgent.ts` — 공통 실행 루프, useExactTools 분기
- `src/utils/forkedAgent.ts` — subagent context 생성, 캐시 메트릭

## Phase 구분

- **Phase 1**: Fork 핵심 동작 (컨텍스트 공유, 메시지 구성, 재귀 방지, worktree 통합)
- **Phase 2**: 캐시 최적화 (rendered_system_prompt 스레딩, useExactTools, 캐시 메트릭)

## 모듈 구조

```
src/
  tools/
    agent_tool.rs          — fork vs normal 분기 결정, 파라미터 조립
  services/
    agent_runtime.rs       — 공통 실행 (foreground/background/worktree), context 빌딩
    fork_subagent.rs (신규) — buildForkedMessages(), buildChildMessage(), 재귀 guard, 상수
```

대응 관계:

| TypeScript | Rust |
|---|---|
| `AgentTool.tsx` (분기 결정) | `agent_tool.rs` |
| `forkSubagent.ts` (메시지 구성) | `fork_subagent.rs` |
| `runAgent.ts` (실행 루프) | `agent_runtime.rs` |
| `forkedAgent.ts` (context/metrics) | `agent_runtime.rs` 내 통합 |

## 데이터 흐름

```
AgentTool.execute(ctx)
  │
  ├── fork 여부 판단
  │   subagent_type 없음 + settings fork_subagent=true + 재귀 guard 통과
  │
  ├── Fork 경로:
  │   1. fork_subagent::build_forked_messages(prompt, &ctx.messages)
  │      → 부모 assistant msg 복제 + placeholder tool_results + directive
  │   2. AgentSpawnParams { is_fork: true, fork_messages, ... }
  │   3. agent_runtime::run_foreground/background(params, ctx)
  │      → build_fork_context() 사용 (부모 system_prompt + 전체 대화 전달)
  │
  └── Normal 경로:
      1. AgentSpawnParams { is_fork: false, ... }
      2. agent_runtime::run_foreground/background(params, ctx)
         → build_child_context() 사용 (기존 로직)
```

## Phase 1: Fork 핵심 동작

### 1-1. fork_subagent.rs

**상수:**

```rust
pub const FORK_PLACEHOLDER: &str = "Fork started — processing in background";
pub const FORK_DIRECTIVE_PREFIX: &str = "Your directive: ";
pub const FORK_BOILERPLATE_TAG: &str = "fork-boilerplate";
pub const FORK_RULES: &str = "...";  // TypeScript의 10개 규칙 포팅
```

**`is_fork_enabled(settings: &Settings) -> bool`**
- `settings.json`의 `fork_subagent: true` 확인
- coordinator mode면 false
- non-interactive session이면 false

**`build_forked_messages(directive: &str, parent_messages: &[ApiMessage]) -> Vec<ApiMessage>`**

부모 대화의 마지막 assistant 메시지를 복제하고, 모든 tool_use 블록에 대해 동일한 placeholder tool_result를 생성:

```rust
// 출력: 2개 메시지
[
  // 1. 부모 assistant 메시지 그대로 복제
  ApiMessage { role: Assistant, content: [original_blocks...] },

  // 2. User 메시지: placeholder results + directive
  ApiMessage { role: User, content: [
    ToolResult { tool_use_id: "id1", content: FORK_PLACEHOLDER },
    ToolResult { tool_use_id: "id2", content: FORK_PLACEHOLDER },
    ...
    Text { text: build_child_message(directive) }
  ]}
]
```

바이트 동일 placeholder가 핵심 — Phase 2에서 캐시 hit의 기반.

**`build_child_message(directive: &str) -> String`**

```rust
fn build_child_message(directive: &str) -> String {
    format!(
        "<fork-boilerplate>\n{rules}\n</fork-boilerplate>\n\n{prefix}{directive}",
        rules = FORK_RULES,
        prefix = FORK_DIRECTIVE_PREFIX,
        directive = directive,
    )
}
```

**재귀 방지 — `is_in_fork_child(messages: &[ApiMessage]) -> bool`**

메시지 내용에서 `<fork-boilerplate>` 태그를 스캔. autocompact 이후에도 작동하는 fallback guard.

```rust
pub fn is_in_fork_child(messages: &[ApiMessage]) -> bool {
    messages.iter().any(|msg| {
        msg.content.iter().any(|block| match block {
            ContentBlock::Text { text } => text.contains(FORK_BOILERPLATE_TAG),
            _ => false,
        })
    })
}
```

**`build_worktree_notice(parent_cwd: &Path, worktree_cwd: &Path) -> String`**

Fork + worktree 조합 시 자식에게 경로 변환과 파일 재읽기를 안내하는 notice.

### 1-2. AgentSpawnParams 확장

```rust
pub struct AgentSpawnParams {
    pub prompt: String,
    pub description: String,
    pub model: Option<String>,
    pub run_in_background: bool,
    pub isolation: Option<String>,
    // Phase 1 신규
    pub is_fork: bool,
    pub fork_messages: Option<Vec<ApiMessage>>,
}
```

### 1-3. agent_tool.rs 변경

`execute()` 안에서 fork 분기:

```rust
async fn execute(&self, ctx: ToolUseContext) -> Result<ToolResult> {
    // 기존 depth check
    if ctx.agent_depth >= 3 {
        return Err("Maximum agent nesting depth (3) reached");
    }

    let subagent_type = params.get("subagent_type").and_then(|v| v.as_str());

    // fork 경로 판단
    let is_fork_path = subagent_type.is_none()
        && fork_subagent::is_fork_enabled(&ctx.settings)
        && !fork_subagent::is_in_fork_child(&ctx.messages);

    let spawn_params = if is_fork_path {
        let fork_msgs = fork_subagent::build_forked_messages(&prompt, &ctx.messages);
        AgentSpawnParams {
            is_fork: true,
            fork_messages: Some(fork_msgs),
            ..base_params
        }
    } else {
        AgentSpawnParams {
            is_fork: false,
            fork_messages: None,
            ..base_params
        }
    };

    if spawn_params.run_in_background {
        run_background(spawn_params, &ctx).await
    } else {
        run_foreground(spawn_params, &ctx).await
    }
}
```

### 1-4. agent_runtime.rs 변경

**build_fork_context() 신규:**

```rust
pub fn build_fork_context(
    params: &AgentSpawnParams,
    parent_ctx: &ToolUseContext,
    tools: Vec<Box<dyn Tool>>,
) -> QueryContext {
    let mut messages = parent_ctx.messages.clone();
    messages.extend(params.fork_messages.clone().unwrap());

    QueryContext {
        messages,
        system_prompt: Some(parent_ctx.system_prompt.clone()),
        tools,
        agent_depth: parent_ctx.agent_depth + 1,
        permission_ctx: parent_ctx.permission_ctx.clone(),
        permission_tx: parent_ctx.permission_tx.clone(),
        ..
    }
}
```

**run_foreground/run_background 분기:**

```rust
pub async fn run_foreground(params: AgentSpawnParams, ctx: &ToolUseContext) -> Result<ToolResult> {
    let tools = filter_tools(ctx, &params);

    let query_ctx = if params.is_fork {
        build_fork_context(&params, ctx, tools)
    } else {
        build_child_context(&params, ctx, tools)
    };

    // 이하 공통: query() 호출, 결과 수집, 반환
}
```

**tool 필터링:**

```rust
fn filter_tools(ctx: &ToolUseContext, params: &AgentSpawnParams) -> Vec<Box<dyn Tool>> {
    if params.is_fork {
        ctx.tools.clone()  // fork: 부모 전체 tool pool
    } else if params.run_in_background {
        ctx.tools.iter().filter(|t| t.is_read_only()).cloned().collect()
    } else {
        ctx.tools.clone()  // foreground normal: all tools
    }
}
```

**worktree + fork 통합:**

worktree isolation이 fork와 함께 사용될 때, worktree notice를 마지막 메시지에 추가:

```rust
if params.isolation.as_deref() == Some("worktree") {
    let worktree_path = create_agent_worktree()?;

    if params.is_fork {
        let notice = fork_subagent::build_worktree_notice(&parent_cwd, &worktree_path);
        query_ctx.messages.last_mut().unwrap()
            .content.push(ContentBlock::Text { text: notice });
    }
}
```

### 1-5. Settings 변경

`settings.json`에 `fork_subagent` 필드 추가:

```json
{
  "fork_subagent": true
}
```

Settings struct에 필드 추가, 기본값 false.

## Phase 2: 캐시 최적화

### 2-1. rendered_system_prompt 스레딩

```rust
pub struct ToolUseContext {
    // 기존 필드...
    pub rendered_system_prompt: Option<Vec<SystemBlock>>,
}
```

흐름:
1. query() 엔진이 API 호출 전 system prompt를 렌더링
2. 렌더링 결과를 ToolUseContext.rendered_system_prompt에 저장
3. build_fork_context()가 이 값을 그대로 사용
4. fallback: None이면 assemble_system_prompt()로 재계산

### 2-2. use_exact_tools

```rust
pub struct AgentSpawnParams {
    // 기존 필드...
    pub use_exact_tools: bool,
}
```

true일 때:
- tool 필터링 스킵 — 부모 tool 직렬화와 동일한 순서/내용 보장
- thinking_config 부모 것 상속
- is_non_interactive_session 부모 것 상속

### 2-3. content_replacement_state 클론

Fork 자식이 부모의 content replacement 상태를 복제:

```rust
// build_fork_context에서
content_replacement_state: parent_ctx.content_replacement_state.clone(),
```

### 2-4. 캐시 메트릭 로깅

```rust
pub struct ForkCacheMetrics {
    pub fork_label: String,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub input_tokens: u64,
    pub cache_hit_rate: f64,
}

fn log_fork_metrics(usage: &Usage, label: &str) {
    let total = usage.input_tokens
        + usage.cache_creation_input_tokens.unwrap_or(0)
        + usage.cache_read_input_tokens.unwrap_or(0);
    let hit_rate = if total > 0 {
        usage.cache_read_input_tokens.unwrap_or(0) as f64 / total as f64
    } else { 0.0 };

    tracing::info!(
        fork_label = label,
        cache_hit_rate = hit_rate,
    );
}
```

## Phase 2 변경 요약

| 변경 | 파일 | 목적 |
|---|---|---|
| `rendered_system_prompt` 필드 | tool.rs, query/engine.rs | 바이트 동일 system prompt |
| `use_exact_tools` 필드 | agent_runtime.rs | tool 직렬화 동일성 |
| content_replacement_state 클론 | agent_runtime.rs | wire 메시지 동일성 |
| 캐시 메트릭 로깅 | agent_runtime.rs | 효과 측정 |

## 테스트 전략

### Phase 1 단위 테스트
- `build_forked_messages()`: 부모 메시지에서 tool_use 추출, placeholder 생성, directive 포함 검증
- `build_child_message()`: fork-boilerplate 태그 포함, directive prefix 검증
- `is_in_fork_child()`: fork-boilerplate 태그 감지, 없을 때 false
- `is_fork_enabled()`: settings 조합별 true/false
- `build_worktree_notice()`: 경로 포함 검증

### Phase 1 통합 테스트
- fork 경로 진입: subagent_type 없음 + flag on → is_fork=true
- 재귀 방지: fork 자식 내부에서 다시 fork 시도 → normal 경로로 fallback
- worktree + fork: notice가 메시지에 추가되는지 검증

### Phase 2 테스트
- rendered_system_prompt 스레딩: query 엔진 → ToolUseContext → fork context 전달 검증
- 캐시 메트릭: usage 값으로 hit rate 계산 검증
