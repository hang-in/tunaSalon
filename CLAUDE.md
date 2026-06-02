# tunaSalon — Claude Code Handoff Document

## 1. Project Overview

tunaSalon는 로컬 LLM 페르소나들을 한 방에서 스몰토크하게 만드는 Rust TUI 앱입니다. 핵심 산출물은 페르소나가 아니라 그 뒤의 대화 흐름 엔진으로, Hawkes 발화 강도(언제) + RRF 화자 선택(누가) + SilenceGate(말할까) + 임베딩 수렴/발산(계속할까)을 미터로 관찰하며 튜닝합니다. 설계 SSOT는 `docs/reference/salon-engine-design.md`, 실행 플랜은 `docs/plans/salon-engine-v1.md`에 있습니다.

개발은 Architect(계획)·Developer(구현)·Reviewer(검토) 역할 분리로 진행하며(`docs/agents/`), 실제 구현은 Codex 또는 Sonnet 서브에이전트에 위임하고 Claude가 리뷰·검증합니다. **현재 v0.1~v0.3 구현 완료**(2026-06-02): 리듬 엔진(v0.1) + 케미 α(v0.2) + 로컬 LLM PersonaRuntime(v0.3). 72 tests, 스모크 게이트 green, 공개 레포 https://github.com/hang-in/tunaSalon. 다음은 v0.4(동시 호출). 단계 로드맵은 `docs/plans/salon-engine-v1.md`, 단계별 플랜은 `salon-engine-v2.md`(done)·`salon-engine-v3.md`(done).

## 2. 기술 스택

### 산출물 (tunaSalon 제품)

| 계층 | 기술 |
|------|------|
| 소스 언어 | Rust (edition 2021) |
| 앱 형태 | TUI(ratatui + crossterm) + headless 모드. 엔진 코어는 출력 sink와 분리 |
| 결정성 | 주입 ChaCha8Rng + 논리 ts + BTreeMap. 같은 seed → 바이트 동일 NDJSON |
| LLM 백엔드 | Ollama (v0.3 구현). 기본은 LLM off(FakeBackend, 결정적). `--llm`이면 OllamaBackend(reqwest blocking). 기본 모델 `gemma4:e4b`(로컬), cloud는 `<model>:cloud`로 로컬 데몬이 원격 프록시 |
| 임베딩 | BGE-M3 - v0.5(FlowMeter)부터 (초기엔 키워드/유사도 근사) |
| 비밀 | `OLLAMA_CLOUD_API_KEY`는 루트 `.env`(gitignored). `.env.example`만 커밋. 키는 https 원격 엔드포인트의 Authorization 헤더에만 |

> 화자 선택은 엔진이 결정적으로, 발화 내용 생성만 LLM이. 기본 실행은 LLM 없이 v0.1 골든과 바이트 동일(불변식).

### 개발 파이프라인 (tunaFlow, 메타)

| 역할 | 엔진 |
|------|------|
| 워크플로우 엔진 | tunaFlow (자체 파이프라인) |
| Architect (계획) | Claude (Opus) |
| Developer (구현) | Codex (gpt-5-codex) - 문서 기반 추정 |
| Reviewer (검토) | Gemini 2.5 Pro - 문서 기반 추정 |

## 3. 빌드 / 테스트

Rust 프로젝트. 코드베이스 존재(`src/` 모듈: model, sink, hawkes, gate, rrf, utterance, driver, headless, tui, sweep, preset, runtime, ollama).

```bash
cargo test                                       # 전체 72 tests (스모크 게이트 3종: smoke=v0.1, smoke_v2=v0.2, smoke_v3=v0.3)
cargo run                                        # TUI (DebugMeter, 인터랙티브). q 종료, space 일시정지
cargo run -- --headless --seed 42 --ticks 200    # 결정적 NDJSON (틱당 한 줄)
cargo run -- --sweep                             # θ×k 격자 + room preset 비교
cargo run -- --room argument --fsm               # 케미 프리셋 + 같은 화자 2연속 금지
cargo run -- --llm --model gemma4:e4b            # 로컬 LLM 발화 생성(opt-in). cloud는 --model <model>:cloud
cargo run --example persona_collapse             # 같은 모델·다른 페르소나 출력 비교(라이브 Ollama 필요)
```

**골든 회귀 검증 주의**: `cargo run > file` 출력을 비교할 때는 `cargo build` 후 명시적 순차 실행으로. for-loop 안 `cargo run`은 첫 실행이 재빌드되며 빈 출력이 나와 거짓 회귀로 오인된다(이 세션에서 반복 겪음).

## 4. 코딩 컨벤션

- 요청된 부분만 수정. 주변 코드 정리 금지
- 개발 중 silent fallback 최소화
- 투기적 추상화·미래 대비 설계 금지
- 한 경로씩 수정 → 검증 → 다음으로 진행
- 파일명: 2~4 핵심 토큰, camelCase
- 문서 상단 메타데이터 필수: `type`, `status`, `updated_at`
- 에이전트 간 메시지 소유권 주장 금지
- 백그라운드 명령 실행 금지 (`&`, `nohup`, `disown`)

## 5. 다음 우선순위

- **v0.4 (동시 호출)**: 백엔드 풀 + max_concurrent 세마포어(ollama cloud 상한 3) + 큐 + 타임아웃 폴백. e4b 3개 병렬은 persona collapse 비교·burst 표현용. v0.4부터 async(tokio) 검토(v0.3은 blocking reqwest). 로드맵: `docs/plans/salon-engine-v1.md` §3.
- **외부 미결(본격 cloud 사용 전 필수)**: ollama.com 가격 페이지에서 cloud prompt caching 지원 + 과금 단위 확인. 캐시 없으면 발화마다 누적 로그가 선형 과금.
- **미룸(설계 노트만 있음, `docs/temp/`)**: friend engine(장기기억, 회상 슬롯은 v0.3 인터페이스에 예약됨), KV 캐시 최적화(느림 측정 후), 페르소나 40조각 on-demand 조립.
- 세션 운영: 구현 위임은 Codex(`-c 'mcp_servers={}'` 필수, 안 그러면 hang) 또는 Sonnet 서브에이전트. 메모리 참조: `~/.claude/projects/.../memory/`.

---

> Auto-detected by tunaFlow. 내용을 검토하고 필요하면 수정하세요.