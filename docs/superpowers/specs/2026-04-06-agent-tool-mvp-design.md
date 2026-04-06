# AgentTool MVP Design Spec

## Overview

AgentTool의 `execute()`를 실제 서브에이전트 스폰으로 구현한다. 기존 `query()` 엔진을 재사용하여 자식 에이전트가 독립적인 LLM 대화를 수행하고 결과를 부모에게 반환한다.

**MVP 범위:** Foreground sync + Background async
**제외:** Remote, Worktree isolation, Fork subagent, Team/Coordinator

---

## 1. 실행 흐름

```
AgentTool.execute(ctx)
  │
  ├─ prompt, description, model, run_in_background 파라미터 추출
  ├─ agent_depth 체크 (>= 3이면 에러 반환)
  │
  ├─ 자식 QueryContext 구성
  │   ├─ messages: [user(prompt)]  — 시스템 프롬프트는 부모 것 그대로
  │   ├─ tools: get_agent_tools(is_background) — fresh 인스턴스 생성
  │   ├─ provider: model override 있으면 부모 config + 새 model, 없으면 부모 것
  │   ├─ permission_ctx: 부모 규칙 복사 (상속)
  │   ├─ permission_tx/question_tx: 부모 채널 그대로 (버블업)
  │   ├─ max_turns: 50
  │   └─ max_budget_usd: None (부모 예산에서 차감)
  │
  ├─ [foreground] query(child_ctx, parent_event_tx, child_abort) 호출
  │   ├─ parent_event_tx 직접 전달 (SkillTool 패턴)
  │   ├─ child_abort = parent abort의 child token
  │   ├─ 완료 대기
  │   └─ assistant 응답 텍스트를 ToolResult로 반환
  │
  └─ [background] tokio::spawn으로 비동기 실행
      ├─ BackgroundTask로 AppState.background_tasks에 등록
      ├─ 즉시 ToolResult 반환: { status: "background_started", agent_id }
      ├─ 완료 시:
      │   ├─ BackgroundTask status=Completed, output에 결과 저장
      │   └─ QueryEvent::AgentCompleted { agent_id, result } 전송
      └─ 실패 시:
          ├─ BackgroundTask status=Failed, output에 에러 저장
          └─ QueryEvent::AgentCompleted { agent_id, error } 전송
```

---

## 2. 새 모듈

### 2.1 `src/services/agent_runtime.rs`

서브에이전트 실행의 핵심 로직. 단일 모듈로 충분 — 별도 registry 불필요 (기존 `BackgroundTask` 재사용).

```rust
pub struct AgentSpawnParams {
    pub prompt: String,
    pub description: Option<String>,
    pub model: Option<String>,
    pub run_in_background: bool,
    pub subagent_type: Option<String>,
}

pub struct AgentResult {
    pub agent_id: String,
    pub output: String,
    pub is_error: bool,
}

/// 에이전트용 도구 세트를 fresh로 생성한다.
/// Box<dyn Tool>은 Clone 불가이므로 get_builtin_tools()로 새로 만들고 필터링.
pub fn get_agent_tools(is_background: bool) -> Vec<Box<dyn Tool>> {
    let all = crate::tools::get_builtin_tools();
    if !is_background {
        return all;  // foreground: 전체 도구
    }
    all.into_iter()
        .filter(|t| BACKGROUND_ALLOWED_TOOLS.contains(&t.name()))
        .collect()
}

/// 자식 QueryContext를 구성한다.
/// parent_ctx에서 provider, permissions, channels, cwd 등을 가져오고
/// tools는 get_agent_tools()로 fresh 생성.
pub fn build_child_context(
    params: &AgentSpawnParams,
    parent_ctx: &ToolUseContext,
    event_tx: mpsc::UnboundedSender<QueryEvent>,
    abort_signal: CancellationToken,
) -> QueryContext;

/// Foreground: query() 호출 후 결과 반환
pub async fn run_foreground(
    params: AgentSpawnParams,
    parent_ctx: &ToolUseContext,
) -> Result<AgentResult>;

/// Background: tokio::spawn + BackgroundTask 등록
pub async fn run_background(
    params: AgentSpawnParams,
    parent_ctx: &ToolUseContext,
) -> Result<ToolResult>;
```

---

## 3. 도구 필터링

```rust
const BACKGROUND_ALLOWED_TOOLS: &[&str] = &[
    "Read", "Glob", "Grep", "WebFetch", "WebSearch",
    "ToolSearch", "TodoWrite",
    "ListMcpResources", "ReadMcpResource",
];
```

**Foreground:** `get_builtin_tools()` 전체 (Agent 재귀 포함)
**Background:** 위 목록만. Bash 제외 — 파일 수정/삭제 가능하므로 읽기 전용 원칙에 맞지 않음.

> Note: MCP 도구는 MVP에서 제외. 부모의 `mcp_manager`를 자식에 전달하면 MCP 도구도 추가할 수 있으나, `Box<dyn Tool>` 생성이 async이므로 별도 처리 필요. 향후 확장.

---

## 4. 퍼미션 처리

- 부모의 `permission_ctx` (allow_rules, deny_rules)를 자식에게 복사
- 부모의 `permission_tx` 채널을 그대로 전달 → 새 퍼미션 요청은 부모 TUI에 표시
- 기존 허용 규칙에 매칭되면 자동 허용 (기존 `check_permission()` 로직이 처리)

---

## 5. 재귀 깊이 제한

`ToolUseContext`에 `agent_depth: u32` 필드를 추가한다.

- 최초 쿼리: `agent_depth = 0`
- 서브에이전트 스폰 시: 자식에게 `parent_depth + 1` 전달
- `AgentTool.execute()`에서 `ctx.agent_depth >= 3`이면 에러 반환:
  ```
  "Maximum agent nesting depth (3) reached. Cannot spawn further sub-agents."
  ```

**변경 파일:** `src/tool.rs` (ToolUseContext에 필드 추가), `src/query/engine.rs` (execute_tool에서 전달)

---

## 6. QueryEvent 확장

```rust
pub enum QueryEvent {
    // ... 기존 variants ...

    /// Background 에이전트 완료/실패 알림
    AgentCompleted {
        agent_id: String,
        description: String,
        result: Option<String>,
        is_error: bool,
    },
}
```

TUI에서 `AgentCompleted` 수신 시:
1. 상태바에 "Agent '{description}' completed" 표시
2. 결과를 `AppState.pending_agent_results: Vec<(String, String)>`에 저장

**결과 주입 타이밍:** 다음 유저 쿼리 제출 시 `submit_query()`에서 `pending_agent_results`를 drain하고, api_messages에 시스템 메시지로 prepend:
```
[Background agent '{description}' completed with result: {result}]
```

> Note: 현재 진행 중인 query loop에는 주입하지 않는다. 엔진이 spawned task에서 독립 실행 중이므로 메시지 목록을 외부에서 수정할 수 없다.

---

## 7. AgentTool.execute() 변경

```rust
async fn execute(&self, ctx: ToolUseContext) -> Result<ToolResult> {
    // 깊이 체크
    if ctx.agent_depth >= 3 {
        return Ok(ToolResult {
            content: "Maximum agent nesting depth (3) reached.".into(),
            is_error: true,
            content_blocks: None,
        });
    }

    let params = AgentSpawnParams::from_input(&ctx.input)?;

    if params.run_in_background {
        agent_runtime::run_background(params, &ctx).await
    } else {
        let result = agent_runtime::run_foreground(params, &ctx).await?;
        Ok(ToolResult {
            content: result.output,
            is_error: result.is_error,
            content_blocks: None,
        })
    }
}
```

---

## 8. 이벤트 전달 (MVP 단순화)

SkillTool 패턴을 따라 부모의 `event_tx`를 자식 query()에 직접 전달한다.

- 자식의 `TextDelta`, `ToolCallStart/Complete`, `Usage` 등이 부모 TUI에 그대로 표시
- 부모 vs 자식 구분은 MVP에서 하지 않음 (Phase 2에서 프록시 레이어 추가)

---

## 9. 시스템 프롬프트

부모의 `system_prompt: Vec<SystemBlock>`을 그대로 자식에게 전달한다 (SkillTool 패턴).

프로젝트 컨텍스트(AGENTS.md, git info 등)가 이미 포함되어 있으므로 별도 조합 불필요.

---

## 10. 모델 오버라이드

`model` 파라미터가 지정된 경우:
- 부모 provider의 설정(API key, base URL)은 유지
- model 필드만 교체하여 자식 QueryContext.model에 설정
- provider 자체는 부모 것 그대로 사용 (provider가 model을 request에서 받으므로)

> `CreateMessageRequest.model`에 모델명이 들어가므로 provider를 새로 만들 필요 없음. QueryContext.model만 바꾸면 됨.

---

## 11. 취소 처리

- **Foreground:** 부모 `abort_signal`의 child token 생성 → 부모 취소 시 자식도 취소
- **Background:** 독립 `CancellationToken` → `BackgroundTask.cancel_token`에 저장
- 부모 세션 종료 시 `AppState.background_tasks`의 모든 Running 태스크 cancel

---

## 12. 제약 사항 및 향후 확장

**MVP 제약:**
- Foreground 에이전트는 부모의 tool execution slot을 블로킹 (부모가 동시에 여러 도구 호출하면 순차 실행)
- MCP 도구는 자식 에이전트에서 사용 불가 (builtin만)
- Background 에이전트끼리 파일 충돌 없음 (쓰기 도구 제외됨)
- Background 결과는 현재 query가 아닌 다음 유저 쿼리에서 주입

**향후 확장 (Phase 2+):**
- Worktree isolation: git worktree 격리 실행
- Fork subagent: 부모 컨텍스트 공유 경량 포크
- Team/Coordinator: 멀티에이전트 오케스트레이션
- SendMessage: 에이전트 간 메시지 전달
- 에이전트별 MCP 서버 초기화
- 이벤트 프록시 레이어 (부모/자식 구분)
- Bash를 background에 추가 (read-only 모드 제한과 함께)

---

## 13. 파일 변경 목록

| 파일 | 변경 |
|------|------|
| `src/services/agent_runtime.rs` | **신규** — 서브에이전트 실행 로직 + 도구 필터링 |
| `src/services/mod.rs` | 모듈 추가 |
| `src/tools/agent_tool.rs` | execute() 실제 구현 |
| `src/tool.rs` | ToolUseContext에 `agent_depth: u32` 추가 |
| `src/query/engine.rs` | execute_tool()에서 agent_depth 전달 |
| `src/query/types.rs` | `QueryEvent::AgentCompleted` 추가 |
| `src/state/app_state.rs` | `pending_agent_results` 필드 추가 |
| `src/tui/app.rs` | `AgentCompleted` 이벤트 처리 |
| `src/tui/run.rs` | submit_query()에서 pending results 주입 |

---

## 14. 테스트 전략

| 테스트 | 검증 항목 |
|--------|-----------|
| `test_foreground_agent` | query() 호출, 결과 반환 |
| `test_background_agent` | 즉시 반환, BackgroundTask 등록, 완료 이벤트 |
| `test_tool_filtering` | foreground=전체, background=읽기전용 |
| `test_permission_inheritance` | 부모 규칙 상속, 새 건 버블업 |
| `test_cancellation` | foreground=부모 취소 전파, background=독립 취소 |
| `test_max_depth` | 깊이 3 초과 시 에러 반환 |
| `test_model_override` | 자식 QueryContext.model이 오버라이드됨 |
| `test_background_result_injection` | pending_agent_results → 다음 쿼리에 주입 |
