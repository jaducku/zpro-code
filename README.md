# atom

오픈 LLM 기반 코딩 어시스턴트 CLI. Rust로 작성.

멀티 프로바이더 지원 (OpenRouter, Codex, Ollama, OpenAI 호환 엔드포인트), 인터랙티브 TUI 스트리밍, 도구 실행, 퍼미션 시스템, 확장 가능한 스킬/플러그인 아키텍처.

## 아키텍처

```
atom
├── src/
│   ├── main.rs                     # CLI 진입점 (clap), 프로바이더 설정, 온보딩
│   ├── lib.rs                      # 라이브러리 루트
│   │
│   ├── entrypoints/
│   │   ├── print.rs                # --print 비대화형 모드
│   │   └── setup.rs                # 첫 실행 온보딩 위자드
│   │
│   ├── tui/                        # 터미널 UI (ratatui + crossterm)
│   │   ├── app.rs                  # 앱 상태, 입력 처리, 타입어헤드
│   │   ├── run.rs                  # 이벤트 루프 (EventStream + tokio::select!)
│   │   ├── ui.rs                   # 렌더링 (메시지, 상태바, 다이얼로그)
│   │   └── event.rs                # 이벤트 타입
│   │
│   ├── query/                      # LLM 대화 엔진
│   │   ├── engine.rs               # 쿼리 루프: LLM → 도구 → 퍼미션 → 반복
│   │   └── types.rs                # QueryContext, QueryEvent, PermissionAsk
│   │
│   ├── commands/                   # ~110개 슬래시 커맨드
│   │   ├── types.rs                # CommandMeta, 열거형
│   │   ├── traits.rs               # PromptCommand, LocalCommand, TuiCommand
│   │   ├── registry.rs             # CommandRegistry (빌트인 + 동적)
│   │   ├── prompt/                 # LLM 프롬프트 커맨드 (/review, /commit, ...)
│   │   ├── local/                  # 텍스트 반환 커맨드 (/clear, /exit, ...)
│   │   └── tui_cmds/              # 인터랙티브 TUI 커맨드 (/help, /config, /model, ...)
│   │
│   ├── services/
│   │   ├── api/                    # LLM 프로바이더 추상화
│   │   │   ├── provider.rs         # LlmProvider 트레잇
│   │   │   ├── openai_provider.rs  # OpenAI 호환 (SSE 스트리밍 + 폴백)
│   │   │   ├── provider_factory.rs # 멀티 프로바이더 팩토리
│   │   │   └── codex_auth.rs       # Codex CLI OAuth 토큰 리더
│   │   ├── config/                 # ~/.atom/config.json 관리
│   │   ├── context/                # AGENTS.md + git 컨텍스트 조립
│   │   ├── compact/                # 자동 압축 (토큰 버짓)
│   │   ├── hooks/                  # PreToolUse/PostToolUse 쉘 훅
│   │   ├── keychain/               # API 키 암호화
│   │   ├── mcp/                    # Model Context Protocol 클라이언트
│   │   ├── session_transcript/     # 세션 저장/복원
│   │   ├── skills/                 # SKILL.md 동적 로딩
│   │   └── token_estimation/       # 토큰 수 추정
│   │
│   ├── tools/                      # 빌트인 도구
│   │   ├── bash_tool.rs            # 쉘 명령 실행
│   │   ├── file_read_tool.rs       # 파일 읽기
│   │   ├── file_write_tool.rs      # 파일 생성
│   │   ├── file_edit_tool.rs       # 파일 편집
│   │   ├── glob_tool.rs            # 파일 패턴 매칭
│   │   └── grep_tool.rs            # 코드 검색
│   │
│   ├── utils/
│   │   ├── paths.rs                # 중앙화된 경로 상수 (~/.atom/)
│   │   └── permissions/            # 퍼미션 체커 (5모드 + 자동 분류)
│   │
│   └── types/                      # 공유 타입 (메시지, 콘텐츠, 퍼미션)
│
├── docs/
│   ├── retrospective/              # 세션 회고록
│   └── superpowers/
│       ├── specs/                   # 설계 명세
│       └── plans/                   # 구현 계획
│
└── Cargo.toml
```

## 주요 기능

### LLM 프로바이더
- **OpenRouter** — 멀티 모델 게이트웨이, 무료 모델 지원
- **Codex** — OpenAI Codex CLI OAuth 토큰 재사용
- **Ollama** — 로컬 모델
- **Custom** — 모든 OpenAI 호환 엔드포인트

자동 폴백: system role 미지원 → user 메시지 주입, tool use 미지원 → 도구 없이 재시도.

### 인터랙티브 TUI
- 실시간 스트리밍 응답 표시
- `/` 슬래시 커맨드 타입어헤드 (퍼지 검색)
- `@` 파일 멘션 (프로젝트 파일 피커)
- 오른쪽 드로워 — TUI 커맨드용 (/config, /model, /mcp, ...)
- 퍼미션 다이얼로그 (허용 / 항상 허용 / 거부)
- 한글/CJK 입력 지원

### 도구
6개 빌트인 도구 + MCP 서버 도구:
- **Bash** — 쉘 명령 실행
- **Read** — 파일 읽기
- **Write** — 파일 생성
- **Edit** — 파일 편집 (문자열 치환)
- **Glob** — 파일 패턴 검색
- **Grep** — 코드 내용 검색

### 퍼미션 시스템
5가지 모드:
| 모드 | 동작 |
|------|------|
| **Auto** (기본값) | Read/Grep 자동 허용, Bash/Write는 물어봄 |
| **Default** | 모든 도구 호출마다 물어봄 |
| **AcceptEdits** | 편집 자동 허용, Bash는 물어봄 |
| **Plan** | 모든 도구 실행 차단 (계획만) |
| **Bypass** | 전부 허용 |

### 컨텍스트 & 메모리
- **AGENTS.md** — 오픈 표준 프로젝트 지침 (CLAUDE.md 대체)
- **Git 컨텍스트** — 브랜치, 최근 커밋, 작업 디렉토리
- **세션 저장** — 자동 저장, `--continue`, `--resume`
- **SKILL.md** — `.agents/skills/`에서 동적 스킬 로딩

### 확장성
- **MCP 서버** — `.mcp.json` 설정, `atom mcp add-json/list/remove`
- **쉘 훅** — `.agents/hooks.json` (PreToolUse, PostToolUse)
- **스킬** — `.agents/skills/my-skill/SKILL.md` (YAML 프론트매터)

## 설치

```bash
cargo install --path .
```

바이너리: `~/.cargo/bin/atom` | 설정: `~/.atom/`

## 빠른 시작

```bash
# 첫 실행 — 대화형 온보딩 위자드
atom

# 또는 수동 설정
atom config set provider openrouter
atom config set api_key sk-or-v1-...
atom config set default_model fast
atom config set fast google/gemma-3-27b-it:free
atom config set smart anthropic/claude-sonnet-4-20250514
```

## 사용법

### 대화형 모드
```bash
atom                              # 설정된 프로바이더로 TUI 시작
atom --model smart                # 모델 오버라이드
atom --permission-mode default    # 매번 도구 실행 물어봄
atom --dangerously-skip-permissions  # 퍼미션 전부 건너뛰기
atom --continue                   # 마지막 세션 이어하기
atom --resume SESSION_ID          # 특정 세션 복원
```

### 비대화형 모드
```bash
atom -p "이 코드 설명해줘"
atom -p "main.rs 리팩토링해줘" --output-format json
echo "TODO 주석 전부 찾아줘" | atom -p
atom -p "버그 수정해줘" --max-turns 5 --max-budget-usd 0.50
```

### 슬래시 커맨드 (TUI 내)
```
/help          사용 가능한 커맨드 표시
/model         모델 전환 (대화 중 핫스왑)
/config        설정 확인
/mcp           MCP 서버 관리
/memory        AGENTS.md 내용 보기
/skills        로드된 스킬 목록
/session       저장된 세션 목록
/resume        세션 복원
/diff          git diff 보기
/branch        git 브랜치 목록
/status        시스템 상태
/doctor        진단 실행
/clear         대화 초기화
/exit          종료
```

### CLI 서브커맨드
```bash
atom config show|set|get|path     # 설정 관리
atom mcp list|add-json|get|remove # MCP 서버 관리
atom doctor                       # 헬스 체크
atom --help                       # 전체 CLI 레퍼런스
```

## 설정

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

API 키는 config 파일에 암호화되어 저장됩니다 (머신별 XOR + base64).

### 프로젝트 레벨 설정
```
your-project/
├── AGENTS.md                 # 프로젝트 지침 (자동 로드)
├── .mcp.json                 # 프로젝트 MCP 서버
└── .agents/
    ├── skills/               # 프로젝트 스킬
    │   └── my-skill/
    │       └── SKILL.md
    └── hooks.json            # 프로젝트 훅
```

### 글로벌 설정
```
~/.atom/
├── config.json               # 프로바이더, 모델, 설정
├── sessions/                 # 저장된 세션
├── skills/                   # 글로벌 스킬
├── hooks.json                # 글로벌 훅
├── .mcp.json                 # 글로벌 MCP 서버
└── AGENTS.md                 # 글로벌 지침
```

## 기술 스택

| 구성 요소 | 기술 |
|-----------|------|
| 언어 | Rust |
| TUI | ratatui + crossterm |
| 비동기 | tokio |
| HTTP | reqwest (스트리밍) |
| CLI | clap (derive) |
| 직렬화 | serde + serde_json + serde_yaml |
| MCP | JSON-RPC + stdio 트랜스포트 |

## 라이선스

MIT
