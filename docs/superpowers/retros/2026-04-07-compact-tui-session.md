# 2026-04-07 Session Retro: /compact + Gemma4 + TUI 개선

## 무엇을 했나

33개 커밋. 3개 주요 기능 + 1개 버그 수정.

### 1. /compact + Session Memory (12 커밋)

**문제:** `/compact`와 `/clear`가 동일하게 동작 — 둘 다 컨텍스트를 전부 삭제. 실제 LLM 요약 없음.

**구현:**
- Session memory 서비스 — 백그라운드 fork subagent로 대화 요약을 마크다운 파일로 추출
- `/compact` 파이프라인: session memory compaction (API 호출 없음) → microcompact → full compact (LLM 9-section 프롬프트)
- Auto-compact: REPL 레벨에서 query 완료 후 threshold 기반 자동 실행
- CommandContext 확장 — provider, messages, session_memory 접근
- `/clear`와 `/compact` 분리 (LocalCommandResult::Clear)

**원본 TS 충실도:**
- 구조화된 9-section 프롬프트 verbatim 포팅
- `<analysis>` scratchpad 제거 + `<summary>` 포맷팅
- PTL (Prompt-Too-Long) 재시도 (최대 3회)
- 이미지 블록 제거 (compact 전)
- Summary role: User (Assistant가 아님)
- Session memory: 템플릿, section 분석, truncation

### 2. Gemma4 Reasoning 수정 (2 커밋)

**문제:** Gemma4 모델에서 대화가 아무 응답 없이 끊김.

**조사 과정:**
1. SSE 파서 분석 → `reasoning` 필드 미처리 확인
2. OpenRouter의 reasoning 파라미터 체계 조사
3. Gemma4의 `<|channel>thought...<channel|>` 태그 구조 발견
4. OpenRouter가 `reasoning` 파라미터 요청 시 자동 파싱해준다는 것 확인

**수정:**
- `OpenAiDelta`에 `reasoning` 필드 추가 (DeepSeek의 `reasoning_content`와 별도)
- `OpenAiRequest`에 `reasoning: Option<ReasoningConfig>` 추가
- config.json에 `model_options.reasoning: true` 설정 지원
- Registry에서 config → provider로 reasoning 설정 전파
- 에러 표시 수정: `needs_redraw` + `loading_label` 클리어

**교훈:** 모델별 thinking 태그를 직접 파싱하지 않고, provider(OpenRouter)에 파싱을 위임하는 것이 multi-LLM 시스템에 맞는 접근.

### 3. TUI 활동 표시 개선 (10+ 커밋)

**문제:** 원본 Claude Code에 비해 TUI가 허전 — 도구 활동, thinking, task 진행상황이 안 보임.

**구현:**
- **도구 활동 트리**: Agent 아래 `├─ Bash(find...)`, Running... 상태, +N more (ctrl+o), 전체 인자 표시
- **agent_type**: Explore/Agent/Skill 구분 헤더
- **parent_agent_id**: subagent 이벤트 릴레이로 중첩 관계 전달
- **상태 바**: `✦ {activity_label}… ({elapsed} · ↓ {tokens})` — 원본과 동일
- **Reasoning 표시**: `◐ Thinking… (1.2k tokens)` 스피너 + 접기/펼치기 + 내용 보기
- **TodoWrite 도구**: LLM이 ■/□/✓ task 체크리스트 관리

## 잘된 점

- **원본 조사 철저**: TS 레퍼런스 코드를 직접 읽고 함수 단위로 대조
- **Subagent-driven 구현**: Task별 독립 subagent → 빠른 병렬 진행, 컨텍스트 오염 없음
- **설계 → 계획 → 구현 프로세스**: brainstorm → spec → plan → execute 흐름이 효과적
- **multi-LLM 설계**: Gemma4 수정에서 하드코딩 대신 config 기반 reasoning 위임 채택

## 아쉬운 점

- **Gemma4 디버깅 시간**: 처음에 `<channel>` 태그 파싱을 직접 구현하려 했다가 되돌림. OpenRouter 위임 방식을 먼저 조사했으면 시간 절약
- **에러 표시 누락**: `needs_redraw` 빠진 것을 사용자 보고로 발견 — 에러 경로 테스트 부족
- **TUI smoke test 부재**: TUI 변경사항의 실제 화면 확인이 CLI 환경 제약으로 어려움

## 기술적 결정

| 결정 | 근거 |
|------|------|
| Session memory를 별도 서비스로 | 원본 TS 구조와 동일, extraction은 비동기 + compact는 동기 |
| Auto-compact를 REPL 레벨로 이동 | 원본은 query engine 안이 아닌 REPL에서 처리 |
| Reasoning 파라미터 config 기반 | multi-LLM에서 모델별 thinking 방식을 코드에 하드코딩하면 유지보수 불가 |
| DisplayMessage에 parent_id 추가 | 중첩 도구 표시를 위해 최소 데이터 모델 확장 |
| TodoWrite는 전체 교체 방식 | 원본 Claude Code와 동일, delta 방식보다 단순 |

## 다음 할 일

- RunPod/vLLM에서의 raw thinking 태그 처리 (config-driven tag parsing)
- TUI smoke test 자동화
- 실제 사용 피드백 기반 UX 조정
