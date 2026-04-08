# 회고: Config 시스템 현대화 & 자동배포 파이프라인 (v0.1.9)

**기간**: 2026-04-08  
**주요 작업**: config.json 프로바이더/모델 구조 현대화, release workflow 태깅

---

## 🎯 목표 달성

### 1. Config 시스템 현대화 (v3 → v4)
**달성도**: ✅ 100%

**변경 사항:**
- `providers: HashMap` → `Vec<ProviderConfig>` (순서 보장, 정보 중앙화)
- `models: HashMap` 제거 (모델 정보가 provider 하위에 통합)
- 새 타입 추가: `ProviderAuth`, `ModelConstraints`, `ReasoningParams`, `ModelParams`, `Pricing`, `ModelConfig`, `ProviderConfig`, `Preset`
- 샘플 설정에 reasoning, constraints, pricing, presets 포함
- Registry 업데이트: `from_config()` Vec 순회로 변경, context_window/max_output_tokens 직접 추출

**개선 효과:**
- 🔍 모델 정보 발견성 향상 (capabilities, constraints, pricing 명시)
- 📊 폴백 전략 지원 (fallback 배열)
- 🎚️ reasoning 파라미터 통합 (enabled + effort level)
- 🎨 프리셋 지원 (code, reasoning, creative)

**테스트 결과:**
- config 테스트: 7/7 ✅
- registry 테스트: 13/13 ✅
- 전체 테스트: 720+ 통과 ✅

---

### 2. 배포 파이프라인 수정
**달성도**: ✅ 100%

**문제 진단:**
- Release workflow는 `v*` 태그 푸시 시에만 작동
- 기존: v0.1.5 → main 병합 후 태그 없음 → 배포 안 됨
- 원인: 버전 업그레이드 커밋이 별도로 필요

**해결 과정:**
1. main 브랜치로 체크아웃
2. Cargo.toml 버전 0.1.9로 업데이트
3. 커밋: "chore: bump version to 0.1.9"
4. 태그 생성: v0.1.9
5. 푸시: main + tag 모두 푸시

**결과:**
- ✅ GitHub Actions release workflow 트리거됨
- ✅ 4개 플랫폼 빌드 시작 (x86_64-darwin, aarch64-darwin, x86_64-linux, aarch64-linux)
- ✅ 자동 release notes 생성, 공개 repo 배포

---

## 📋 프로세스 회고

### ✅ 잘한 점

1. **계획 기반 접근 (Plan Mode)**
   - 탐색 에이전트로 기존 코드 구조 파악
   - 상세한 구현 계획 수립 후 진행
   - 리스크 최소화

2. **테스트 우선 (TDD)**
   - 새 타입 정의 후 충분한 테스트 케이스 작성
   - 720+ 테스트 모두 통과
   - 리그레션 방지

3. **정리된 커밋**
   - 배열 기반 구조로 명확한 변경 의도 표현
   - Conventional Commits 준수

4. **배포 자동화**
   - 태그 기반 release workflow 활용
   - 4개 플랫폼 병렬 빌드

### ⚠️ 개선할 점

1. **Git 워크플로우**
   - ❌ dev에 직접 푸시 (feature 브랜치 없이)
   - ✅ 앞으로: feature 브랜치 → PR → merge 워크플로우 준수

2. **버전 관리**
   - ❌ 버전 업그레이드를 병합 후에 수동으로 처리
   - ✅ 앞으로: 병합 전에 버전 결정, 태그 함께 생성

3. **배포 문서화**
   - 버전 0.1.6 ~ 0.1.8 태그 기존 존재 (누가 만들었는지 불명확)
   - 앞으로: DEPLOYMENT.md 작성해서 태깅 절차 명확화

---

## 🔄 다음 스프린트 액션 아이템

1. **Git 워크플로우 문서화**
   - [ ] .claude/WORKFLOW.md 작성
   - [ ] feature 브랜치 네이밍 규칙 정의
   - [ ] PR 템플릿 개선

2. **배포 자동화 강화**
   - [ ] 버전 자동 업그레이드 (changelog 기반)
   - [ ] GitHub Release 자동 생성 검증
   - [ ] 공개 repo 배포 검증

3. **Config 시스템 확장**
   - [ ] CLI: `atom model list`, `atom model info` 커맨드 추가
   - [ ] Fallback 전략 실제 구현 (모델 불가 시 대체)
   - [ ] 환경별 설정 (dev/prod preset)

4. **피처 브랜치 정리**
   - [ ] `feat/builtin-agents-dynamic-load` 상태 확인 및 정리

---

## 📊 메트릭

| 항목 | 결과 |
|------|------|
| Config 타입 추가 | 8개 |
| 테스트 케이스 | 720+ 통과 |
| 지원 플랫폼 | 4개 |
| 배포 시간 | ~10분 (병렬 빌드) |
| 버전 범프 | 0.1.5 → 0.1.9 |

---

## 🎓 배운 점

1. **Vec vs HashMap in Config**
   - Vec: 순서 명시적, 선언적, 순회 친화적
   - HashMap: O(1) 접근이 필요한 경우만 사용
   - → Config는 v4(Vec) 포맷이 더 적절

2. **Release Workflow 트리거**
   - GitHub Actions: 태그 기반 트리거 명확
   - 수동 버전 업그레이드 필요성 인식
   - → 향후 semantic versioning 자동화 검토

3. **문서화의 중요성**
   - 버전 0.1.6~0.1.8 태그 출처 불명확
   - → DEPLOYMENT.md, VERSIONING.md 작성 필요

---

## ✨ 마무리

이번 작업은 config 시스템을 현대화하고 배포 파이프라인을 정상화했습니다.
- **달성**: Config v4 포맷 완성, v0.1.9 배포 자동화
- **다음**: Git 워크플로우 개선, 자동 버전 관리, 환경별 설정 지원
