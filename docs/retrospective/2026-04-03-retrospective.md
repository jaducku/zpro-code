# atom-code 개발 회고록

## 프로젝트 개요

Claude Code CLI (TypeScript, ~520K LOC)를 Rust로 마이그레이션하여 오픈 LLM 기반 코딩 어시스턴트 CLI를 구축한 프로젝트.

**기간:** 2026-04-02 ~ 2026-04-03 (2 세션)
**결과물:** 21,236줄 Rust 코드, 181개 파일, 205개 테스트, 118개 커밋

## 달성한 것

### Phase 0-8: 기반 구조 (세션 1 전반)
- 코어 타입, LLM Provider, Tool 시스템, Query 엔진, MCP 클라이언트, Permission 타입, TUI, CLI 커맨드 (~110개)
- 서브에이전트 기반 병렬 구현으로 빠른 진행

### Phase 9: 서비스 레이어 (세션 1 후반 ~ 세션 2)
- 멀티 프로바이더 (OpenRouter, Codex OAuth, Ollama, 자체 서빙)
- SSE 스트리밍 (수동 파서, reqwest_eventsource 교체)
- Permission 시스템 (5모드, TUI 다이얼로그, Auto classify)
- AGENTS.md 컨텍스트 + git 컨텍스트
- Session 자동 저장 + --continue/--resume
- PreToolUse/PostToolUse 쉘 훅
- 토큰 추적 + 비용 상태바 + auto-compact
- MCP 도구 연동 (.mcp.json → 쿼리 엔진)
- SKILL.md 동적 로딩
- --print 모드 (text/json/stream-json)
- 모델 핫스왑 (/model 커맨드)
- config.json + API 키 암호화 + 온보딩

## 기술적 도전과 해결

### 1. TUI 이벤트 루프와 async 런타임 충돌
**문제:** `crossterm::event::poll()`이 동기 블로킹이라 `tokio::select!`에서 다른 브랜치(query events)를 굶김.

**시도한 것:**
- `try_recv()` 루프 → 타이밍 문제
- `tokio::task::spawn_blocking` → 여전히 select 편향
- `crossterm::EventStream` (최종 해결) → 진정한 async 이벤트

**교훈:** TUI + async는 블로킹 호출을 완전히 제거해야 한다. `event-stream` feature가 핵심.

### 2. OpenAI-compatible API 호환성
**문제:** 모델마다 지원하는 기능이 다름 (system role, tool use).

**해결:** 3단계 자동 fallback:
1. 정상 요청 시도
2. system instruction 에러 → user 메시지로 변환해서 재시도
3. tool use 에러 → 도구 없이 재시도

**교훈:** "OpenAI-compatible"이라도 100% 호환은 아님. 방어적 fallback이 필수.

### 3. UTF-8 멀티바이트 (한글) 입력
**문제:** `cursor_pos`를 바이트 인덱스로 관리하면 한글 입력 시 패닉.

**해결:** `cursor_pos`를 문자 인덱스로 변경, `char_to_byte_pos()` 변환 함수, CJK 더블 위드스 커서 렌더링.

### 4. Permission 다이얼로그 미표시
**문제:** `EventStream::next()`가 키 입력까지 무한 대기 → 다이얼로그가 화면에 안 그려짐.

**해결:** `tokio::select!`에 30ms 주기 틱 브랜치 추가 → 주기적 렌더링 보장.

## 아키텍처 결정

### 원본 대비 변경점
| 항목 | 원본 (TypeScript) | atom-code (Rust) | 이유 |
|------|-------------------|------------------|------|
| 프로젝트 지침 | CLAUDE.md | AGENTS.md | 오픈 표준 |
| 설정 디렉토리 | ~/.claude/ | ~/.atom/ | 독립 브랜드 |
| LLM 프로바이더 | Anthropic only | 멀티 (OpenRouter, Codex, Ollama, custom) | 오픈 LLM 지원 |
| Auto 모드 | ML 분류기 | 규칙 기반 | 외부 의존 제거 |
| API 키 저장 | 평문 | XOR 암호화 | 보안 |
| TUI 프레임워크 | React + Ink | Ratatui + Crossterm | 네이티브 |

### 잘 된 것
- **서브에이전트 기반 병렬 구현**: 독립적인 커맨드/서비스를 병렬로 구현해서 속도 극대화
- **점진적 통합**: Phase별로 독립 테스트 가능한 모듈 구축 → 안정적 진행
- **Fallback 체인**: system/tool 호환성 문제를 자동 재시도로 해결

### 아쉬운 점
- **TUI 이벤트 루프**: 3번 재작성 (poll → spawn_blocking → EventStream). 처음부터 EventStream을 써야 했음
- **하드코딩**: 온보딩 모델, config 경로 등 초기에 하드코딩 후 나중에 수정. 설정 모듈을 먼저 만들었어야
- **테스트 부족**: 단위 테스트 205개지만 통합 테스트 0개. 실제 API 호출 테스트는 수동으로만

## 남은 작업

### 단기
- Permission auto 모드 실사용 검증
- Integration tests
- cargo install 안정화

### 중기
- LSP 통합 (IDE 연결)
- 플러그인 시스템 고도화
- Agent SDK (서브에이전트 스폰)

### 장기
- Phase 10-12 (Auxiliary, Packages, Integration)
- 크로스 플랫폼 (Windows, Linux)
- 배포 자동화

## 수치 요약

```
소스 코드:    21,236줄
파일 수:      181개
테스트:       205개
커밋:         118개
서비스 모듈:  9개
빌트인 도구:  6개
CLI 커맨드:   ~110개
TUI 커맨드:   74개 (40+ 실제 UI)
```
