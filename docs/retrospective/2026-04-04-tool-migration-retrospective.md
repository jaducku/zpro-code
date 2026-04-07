# Tool Migration Retrospective — 2026-04-04

## Overview

하루 만에 Batch 1-7 tool 마이그레이션을 완료했다. 20개 → 36개 tool로 확장. 60 커밋, 79 파일, 17,211줄 추가.

## What We Built

| Batch | Tools | 핵심 내용 |
|-------|-------|----------|
| 1 Core | Sleep, NotebookEdit, WebFetch, WebSearch, AskUserQuestion | readability+htmd, Tavily API, inline prompt |
| 2 Task | TaskCreate/Get/List/Update, TaskStop, TaskOutput, TodoWrite | file-based task_list, BackgroundTask, AppState 확장 |
| 3 Agent Comm | ToolSearch, SendMessage, SyntheticOutput, Brief | deferred tool 시스템, tool_reference, Mailbox, jsonschema |
| 4 Plan/Worktree | EnterPlanMode, ExitPlanMode, EnterWorktree, ExitWorktree | PlanModeService, WorktreeService, git worktree 생명주기 |
| 5 MCP Resources | ListMcpResources, ReadMcpResource | McpConnectionManager accessors, blob persistence |
| 6 Schedule/Config | CronCreate/List/Delete, ConfigTool | CronStore (session+durable), settings registry |
| 7 Team | TeamCreate, TeamDelete | TeamService, 팀 config 관리 |

## Infrastructure Built Along the Way

- **Tool trait 확장:** `should_defer`, `always_load`, `search_hint`, `content_blocks`
- **Deferred tool 시스템:** `is_deferred_tool()`, `ToolDefinition.defer_loading`, `ContentBlock::ToolReference`
- **6개 서비스:** mailbox, plan_mode, worktree, cron, team, mcp (기존 확장)
- **AppState 확장:** agent_message_queues, team_name, output_json_schema, pre_plan_mode, current_worktree_session, cron_store, mcp_manager

## What Went Well

### Subagent-Driven Development가 효과적이었다
- Task별 fresh subagent → context pollution 없음
- 병렬 실행 (worktree isolation) 으로 독립 task 동시 처리
- Spec review → Code quality review 2단계 게이트가 품질 유지

### Brainstorm → Spec → Plan → Execute 파이프라인이 성숙했다
- Batch 3에서 가장 철저하게 진행 (spec 리뷰 2회, 플랜 리뷰 1회)
- 이후 배치들은 패턴이 확립되어 속도가 빨라짐
- Batch 7은 spec + plan + execute를 거의 한 번에 처리

### 원본 TypeScript 코드를 직접 참조하여 정확도 향상
- Explore subagent가 TypeScript 원본을 분석하여 정확한 동작 파악
- 스펙 리뷰에서 원본과 다른 점을 잡아냄 (pre_plan_mode 위치, BriefTool deferral, ToolSearch output 포맷 등)

## What Didn't Go Well

### Worktree 기반 병렬 서브에이전트의 merge 충돌
- 서브에이전트가 오래된 base에서 작업하면 많은 파일이 재수정됨
- Cherry-pick 실패 → 수동으로 핵심 파일만 추출해야 함
- **해결:** worktree agent의 새 파일만 cp로 가져오고, mod.rs 등록은 수동으로 처리

### 서브에이전트가 다른 API 버전으로 코드 작성
- worktree가 old branch base → ToolResult에 content_blocks 없음, ToolUseContext 구조 다름
- ExitPlanMode, ConfigTool에서 수동 수정 필요
- **교훈:** worktree isolation은 독립적인 새 파일에만 효과적. 기존 파일을 많이 수정하는 task에는 부적합

### 리뷰 skip으로 인한 minor 이슈
- 후반 배치에서는 속도를 위해 리뷰를 줄임
- Batch 6-7에서 ConfigTool의 content_blocks 누락, CronList/Delete의 커밋 누락 발생
- **교훈:** 작은 배치라도 최소한의 컴파일 체크는 필요

## Decisions Made

| 결정 | 근거 |
|------|------|
| McpAuthTool 미룸 | McpServerConfig가 stdio만 지원, OAuth에 HTTP/SSE transport 필요 |
| RemoteTriggerTool 미룸 | OAuth 인프라 부재 |
| LSPTool 미룸 | 실험적 (opt-in), LSP 서버 매니저 필요 |
| PowerShellTool 미룸 | Windows 전용, 42KB 보안 엔진, macOS/Linux에서 불필요 |
| SyntheticOutput을 stateful struct로 | 동적 input_schema 필요 (caller의 JSON schema 반환) |
| BriefTool을 always_load으로 | Turn 1에서 바로 사용 가능해야 함, ToolSearch 왕복 불가 |
| ToolSearch output을 tool_reference로 | API가 서버 사이드에서 스키마 주입, XML이 아님 |
| pre_plan_mode를 ToolPermissionContext에 | AppState가 아닌 permission context에 있어야 serde 라운드트립 올바름 |

## Numbers

- **60 커밋** (docs + feat + fix)
- **79 파일 변경**, 17,211줄 추가
- **510 테스트 통과** (4 기존 실패는 skills::tests)
- **36/54 tools** 구현 (67%)
- **7 서비스** 신규 또는 확장
- **7 배치** 완료

## Next Steps

Tool 마이그레이션 핵심은 완료. 남은 작업:
1. **Query engine 개선** — streaming, compaction, turn management
2. **TUI polish** — brief mode 렌더링, worktree/plan mode 표시
3. **Session management** — resume, transcript persistence
4. **AgentTool 실제 구현** — 현재 stub, 실제 서브에이전트 스폰 필요
5. **OAuth 인프라** → McpAuthTool, RemoteTriggerTool 잠금 해제
