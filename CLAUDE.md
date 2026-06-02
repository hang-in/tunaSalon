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

- **v0.4 (동시 호출)**: 이종(heterogeneous) 백엔드 풀 + 백엔드별 max_concurrent 세마포어 + 큐 + 타임아웃 폴백. 실제 백엔드 2종: (1) Ollama Cloud Pro($20/월) 동시성 **3**(서버가 이미 큐잉, 초과 시 거부), (2) 지인서버 `qwen3.6:32b` ctx 100k 동시성 **2**. 동시 호출이 실제로 필요한 곳은 persona collapse 비교/벤치 + burst(같은 맥락 다중 페르소나 동시 반응)이고, 라이브 틱 루프는 발화 1명/틱 + 인과적 턴테이킹이라 순차 유지. **async(tokio)는 비채택 권장**(동시도 ≤5라 std::thread::scope + 세마포어로 충분, blocking reqwest 유지). 로드맵: `docs/plans/salon-engine-v1.md` §3.
- **num_ctx 결정(v0.4)**: 현재 `ollama.rs:96`이 모든 요청에 `num_ctx: 8192` 하드코딩 → cloud auto-max와 지인서버 100k를 우리가 깎아내림. 백엔드별 `Option<u64>`로 전환: None=요청에서 생략(cloud/원격이 모델 최대로 자동 설정), Some(n)=명시(로컬 e4b는 RAM 상한용 8192).
- **외부 미결 → 해소(2026-06-02 조사)**: Ollama Cloud Pro는 **토큰 종량제가 아니라 GPU 사용시간 정액 구독**(Level 1~4 모델 무게 × 요청 길이, 5h 세션·7d 주간 한도로 제한, 깜짝 청구서 없음). "캐시 공유 프롬프트는 GPU 시간 덜 씀" 명시 = 캐시는 작동·budget 절약(종량 초과분 cache-aware pricing은 coming soon). 16,384 ctx 캡은 버그였고 2025-11-14 수정됨(이슈 #13089). **남은 미지수**: 세션/주간 한도의 실제 수치 비공개 + 잔여 budget API 미노출(#15663) → 한도 도달 전엔 모름, 사용자 모니터링으로 커버(e4b면 24/7 무난 판단).
- **미룸(설계 노트만 있음, `docs/temp/`)**: friend engine(장기기억, 회상 슬롯은 v0.3 인터페이스에 예약됨), KV 캐시 최적화(느림 측정 후), 페르소나 40조각 on-demand 조립.
- 세션 운영: 구현 위임은 **Sonnet 서브에이전트**(Agent tool, model sonnet)로. **codex 비사용**(사용자 지시 2026-06-02). Claude(Opus)가 스펙 작성·리뷰·검증. 메모리 참조: `~/.claude/projects/.../memory/`.

---

> Auto-detected by tunaFlow. 내용을 검토하고 필요하면 수정하세요.