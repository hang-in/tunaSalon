# tunaSalon — Claude Code Handoff Document

## 1. Project Overview

tunaSalon는 로컬 LLM 페르소나들을 한 방에서 스몰토크하게 만드는 Rust TUI 앱입니다. 핵심 산출물은 페르소나가 아니라 그 뒤의 대화 흐름 엔진으로, Hawkes 발화 강도(언제) + RRF 화자 선택(누가) + SilenceGate(말할까) + 임베딩 수렴/발산(계속할까)을 미터로 관찰하며 튜닝합니다. 설계 SSOT는 `docs/reference/salon-engine-design.md`, 실행 플랜은 `docs/plans/salon-engine-v1.md`에 있습니다.

이 프로젝트는 tunaFlow 멀티에이전트 워크플로우 파이프라인으로 개발합니다. Architect(계획), Developer(구현), Reviewer(검토) 역할을 분리해 Claude/Codex/Gemini 등 복수 LLM 엔진이 협업하며, 역할 정의는 `docs/agents/`에 있습니다. 현재는 설계와 플랜 v1까지 완료된 상태이고 v0.1 구현은 아직 착수 전입니다.

## 2. 기술 스택

### 산출물 (tunaSalon 제품)

| 계층 | 기술 |
|------|------|
| 소스 언어 | Rust |
| 앱 형태 | TUI (DebugMeter) + headless 모드. 엔진 코어는 출력 sink와 분리 |
| LLM 백엔드 | Ollama (local/cloud), LM Studio - v0.3부터 |
| 임베딩 | BGE-M3 - v0.5부터 (초기엔 키워드/유사도 근사) |
| 문서 관리 | Markdown (docs/plans/, docs/reference/, docs/prompts/) |

> TUI 크레이트는 미정(확인 필요). v0.1은 LLM/임베딩 없이 엔진 코어 + headless만으로 굴립니다.

### 개발 파이프라인 (tunaFlow, 메타)

| 역할 | 엔진 |
|------|------|
| 워크플로우 엔진 | tunaFlow (자체 파이프라인) |
| Architect (계획) | Claude (Opus) |
| Developer (구현) | Codex (gpt-5-codex) - 문서 기반 추정 |
| Reviewer (검토) | Gemini 2.5 Pro - 문서 기반 추정 |

## 3. 빌드 / 테스트

Rust 프로젝트입니다. v0.1 구현 착수 전이라 아직 `Cargo.toml`은 없으며, 구현 시 아래 명령이 표준이 됩니다.

```bash
cargo build
cargo test --test smoke                          # 결정적 스모크 (고정 seed, v0.1 완료 기준 자동 검증)
cargo run -- --headless --seed 42 --ticks 200    # headless 수동 관찰 (NDJSON 출력)
cargo run                                        # TUI (DebugMeter, 인터랙티브)
```

headless 모드는 TUI 없이 엔진을 고정 seed로 돌려 결정적 출력(NDJSON)을 내보내므로, 사람과 에이전트(Reviewer 포함) 모두 스모크 테스트로 직접 검증할 수 있습니다.

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

- v0.1 구현 (MVP): HawkesEngine(μ, self-decay) + SilenceGate(θ) + RRF(시간·균형·난수) + fake utterance + DebugMeter. 엔진 코어와 출력 sink를 분리하고, headless 러너 + 고정 seed 스모크 테스트를 함께 만든다.
- 작업 지시서 분해: `docs/plans/salon-engine-v1.md`의 v0.1 작업 항목을 task 문서로 떼어낸다.

---

> Auto-detected by tunaFlow. 내용을 검토하고 필요하면 수정하세요.