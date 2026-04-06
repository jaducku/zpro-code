# Prompt Hooks 연결 Design Spec

## Overview

`executor.rs`의 Prompt 훅 스텁을 실제 LLM 호출로 구현한다. `HookContext` 구조체를 도입하여 provider를 훅 실행 파이프라인에 전달하고, `query_without_streaming` 헬퍼로 단일 LLM 호출 후 `{"ok": bool, "reason": "..."}` 응답을 파싱한다.

**범위:** Prompt 훅만. Agent 훅은 별도 작업.

---

## 1. HookContext 구조체

```rust
// src/services/hooks/types.rs

/// 훅 실행에 필요한 런타임 컨텍스트.
/// 향후 Agent 훅에서도 재사용.
pub struct HookContext {
    pub provider: Option<Arc<dyn LlmProvider>>,
}
```

`Option`인 이유: SessionStart 등 provider 초기화 전에 실행되는 훅도 있음. None이면 Prompt/Agent 훅은 NonBlockingError 반환.

---

## 2. 시그니처 변경

### execute_event_hooks

```rust
// 변경:
pub async fn execute_event_hooks(
    event: &str,
    match_query: &str,
    input: &serde_json::Value,
    settings: &HooksSettings,
    cwd: &Path,
    hook_ctx: &HookContext,
) -> HookExecutionResult
```

### execute_single_hook

```rust
// 변경:
async fn execute_single_hook(
    hook: &HookCommand,
    env: &HookEnv,
    cwd: &Path,
    hook_ctx: &HookContext,
) -> HookExecutionResult
```

### 기존 테스트 업데이트

executor.rs에 7개 기존 테스트가 `execute_event_hooks()`를 호출함. 모두 `&HookContext { provider: None }`을 추가 인자로 전달해야 함. Command/HTTP 훅 테스트이므로 provider None이 정상.

---

## 3. ALWAYS_EMITTED 이벤트 필터링

기존에 HTTP 훅만 필터링하던 로직을 Prompt/Agent 훅에도 적용:

```rust
// executor.rs, execute_event_hooks 내부:
if ALWAYS_EMITTED.contains(&event) {
    matching_hooks.retain(|hook| matches!(hook, HookCommand::Command { .. }));
}
```

SessionStart/Setup 이벤트에서는 Command 훅만 허용. HTTP, Prompt, Agent 모두 필터.

---

## 4. query_without_streaming 헬퍼

```rust
// src/services/hooks/prompt_hook.rs

pub async fn query_without_streaming(
    provider: &dyn LlmProvider,
    prompt: &str,
    model: Option<&str>,
    timeout_ms: u64,
) -> Result<String, String>
```

**동작:**
1. `CreateMessageRequest` 구성:
   ```rust
   CreateMessageRequest {
       model: model.unwrap_or(&provider.model_info().id).to_string(),
       messages: vec![ApiMessage {
           role: Role::User,
           content: vec![ContentBlock::Text { text: prompt.to_string() }],
       }],
       max_tokens: 4096,
       system: None,
       tools: None,
       temperature: None,
       stream: true,
   }
   ```
2. `provider.create_message(request)` 호출 → `MessageStream` 반환
3. 스트림 순회하며 `StreamEvent::ContentDelta(ContentDelta::TextDelta(text))` 누적
4. `tokio::time::timeout(Duration::from_millis(timeout_ms), ...)` 래핑
5. 타임아웃 시 `Err("Prompt hook timed out")` 반환

---

## 5. JSON 추출 로직 강화

LLM이 JSON 앞뒤에 텍스트를 붙이는 경우 대응. `parse_prompt_response` 수정:

```rust
pub fn parse_prompt_response(response: &str) -> Result<(), String> {
    // 1차: 전체 문자열 파싱 시도
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(response) {
        return check_ok_field(&v);
    }

    // 2차: 줄 단위로 { 로 시작하는 줄 찾아서 파싱
    for line in response.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('{') {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
                return check_ok_field(&v);
            }
        }
    }

    // 파싱 실패 — 인프라 오류이므로 NonBlockingError로 처리 (호출자에서)
    Err(format!("Could not parse JSON from response: {}", 
        &response[..response.len().min(200)]))
}

fn check_ok_field(v: &serde_json::Value) -> Result<(), String> {
    match v.get("ok").and_then(|v| v.as_bool()) {
        Some(true) => Ok(()),
        _ => {
            let reason = v.get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("Blocked by prompt hook");
            Err(reason.to_string())
        }
    }
}
```

---

## 6. Prompt 훅 실행 로직 (에러 매핑 수정)

```rust
HookCommand::Prompt { prompt, model, timeout, .. } => {
    let Some(provider) = &hook_ctx.provider else {
        return non_blocking_error("Prompt hooks require LLM provider (not available)");
    };

    let arguments_json = env.get("ATOM_TOOL_INPUT").cloned().unwrap_or_default();
    let filled_prompt = prompt_hook::build_prompt_input(prompt, &arguments_json);

    let timeout_ms = timeout.unwrap_or(30_000);
    let response = prompt_hook::query_without_streaming(
        provider.as_ref(),
        &filled_prompt,
        model.as_deref(),
        timeout_ms,
    ).await;

    match response {
        Ok(text) => {
            match prompt_hook::parse_prompt_response(&text) {
                Ok(()) => success_result(),
                Err(reason) => {
                    // JSON 파싱 자체가 실패한 경우 → NonBlockingError
                    // ok=false인 경우 → Blocking
                    if reason.starts_with("Could not parse JSON") {
                        non_blocking_error(&reason)
                    } else {
                        blocking_result(&reason)
                    }
                }
            }
        }
        Err(err) => non_blocking_error(&err),
    }
}
```

**에러 매핑 규칙:**
- LLM 호출 실패/타임아웃 → `NonBlockingError` (인프라 문제)
- JSON 파싱 실패 → `NonBlockingError` (인프라 문제)
- `ok=false` → `Blocking { reason }` (정책 결정)
- `ok=true` → `Success`

---

## 7. 호출부 변경

### engine.rs (2곳)

```rust
// 루프 밖에서 한번 생성
let hook_ctx = crate::services::hooks::HookContext {
    provider: Some(ctx.provider.clone()),
};

// PreToolUse (line ~289)
let hook_result = execute_event_hooks(
    "PreToolUse", &tc.name, &tc.input, &ctx.hooks, &ctx.cwd, &hook_ctx,
).await;

// PostToolUse (line ~322)
let hook_result = execute_event_hooks(
    "PostToolUse", &tc.name, &tc.input, &ctx.hooks, &ctx.cwd, &hook_ctx,
).await;
```

---

## 8. Agent 훅 스텁

변경 없음 — `hook_ctx` 파라미터만 시그니처에 추가. 메시지 그대로 유지.

---

## 9. 파일 변경 목록

| 파일 | 변경 |
|------|------|
| `src/services/hooks/types.rs` | `HookContext` 구조체 추가 |
| `src/services/hooks/executor.rs` | 시그니처에 `hook_ctx` 추가, Prompt 분기 구현, ALWAYS_EMITTED 필터 확장, 기존 테스트 7개 업데이트 |
| `src/services/hooks/prompt_hook.rs` | `query_without_streaming` 추가, `parse_prompt_response` 강화 (JSON 추출), `check_ok_field` 헬퍼 |
| `src/services/hooks/mod.rs` | `pub use types::*`가 이미 HookContext를 커버 — 변경 불필요 |
| `src/query/engine.rs` | 훅 호출부에 HookContext 생성 및 전달 (2곳) |

---

## 10. 테스트 전략

| 테스트 | 검증 항목 |
|--------|-----------|
| `test_query_without_streaming_collects_text` | 스트림 TextDelta 수집 |
| `test_query_without_streaming_timeout` | 타임아웃 시 에러 반환 |
| `test_prompt_hook_success` | ok=true → Success |
| `test_prompt_hook_blocking` | ok=false → Blocking |
| `test_prompt_hook_no_provider` | provider None → NonBlockingError |
| `test_prompt_hook_invalid_json` | JSON 파싱 실패 → NonBlockingError |
| `test_prompt_hook_json_with_preamble` | LLM이 텍스트+JSON 반환 → JSON 추출 성공 |
| `test_parse_prompt_response_extracts_json_line` | 줄 단위 JSON 추출 |
| 기존 7개 executor 테스트 | `HookContext { provider: None }` 추가 후 통과 확인 |
