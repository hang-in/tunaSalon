# tunaSalon — Claude Code Handoff Document

## 1. Project Overview

tunaSalon는 **사용자가 LLM 페르소나들과 스몰토크하는 채팅방** Rust TUI 앱입니다(사람=1급 참여자, 페르소나=cloud/friend LLM). 일반 라운드로빈 persona chat과 다른 점은 그 뒤의 **대화 흐름 엔진**(Hawkes 발화 강도=언제 + RRF=누가 + SilenceGate=말할까 + 수렴/발산=계속할까)이 페르소나를 살아있게 만든다는 것 - 욕구가 차오르고·침묵하고·서로 자극하는 리듬. **엔진은 수단(생동감), 제품은 채팅방입니다.**

> 방향 주의(2026-06-02): v0.1~v0.4가 엔진·headless·동시성에 집중하다 정작 제품(채팅 화면·사람 참여)을 못 만들고 실험 하네스로 드리프트했음. v0.5에서 되돌리는 중(사람을 방에 앉힘). headless/결정성/스모크는 제품의 얼굴이 아니라 **엔진 회귀 검증용 dev 도구**로 강등(유지).

설계 SSOT `docs/reference/salon-engine-design.md`(엔진 + §5 사람 참여), **제품 UI 설계 `docs/temp/salon-persona-ui.md` §5**(채팅 pane + 게이지 사이드바 + 입력창), 단계 로드맵 `docs/plans/salon-engine-v1.md`.

개발은 Architect(계획)·Developer(구현)·Reviewer(검토) 역할 분리로 진행하며(`docs/agents/`), 실제 구현은 **Sonnet 서브에이전트에 위임**(codex 비사용, 2026-06-02 사용자 지시)하고 Claude(Opus)가 스펙 작성·리뷰·검증합니다. **현재 v0.1~v0.9 구현 완료**(2026-06-03): 리듬(v0.1) + 케미 α(v0.2) + 로컬 LLM(v0.3) + 동시 호출·이종 백엔드 풀(v0.4) + 사람 참여 채팅방(v0.5) + FlowMeter(v0.6) + MetaController(v0.7) + friend engine 첫 증분(v0.8) + **friend engine 심화(v0.9: 한국어 형태소 Lindera + SQLite/FTS5 BM25 회상 + 세션 넘는 영속, seCall 검색코어 lift, `friend-engine` feature 뒤·골든 무손상) + 생동감 `--chat`(튜닝된 3-way config + `/topic`))**. + **v0.10 진행중(Stage 2a/b/c 완료, task-50 마감 미완)**: friend engine 의미검색 - `OrtEmbedder`(BGE-M3 ONNX **in-process**, `embed.rs`. **실측 viable**: 로드 3.8s/embed 29ms/2.3GB, ollama 아님·랙 없음, ort=download-binaries) + usearch HNSW(`ann.rs`) + hybrid RRF 회상(BM25 어휘+벡터 의미, 참여격리). 새 `friend-engine-semantic` feature 뒤(기본/골든 무손상, recall은 cfg로 v0.9 보존). default 225 / friend-engine 234 / friend-engine-semantic 260 tests, 공개 레포 https://github.com/hang-in/tunaSalon. v2~v9 done. **다음**: ① **v0.10 마감 = task-50**(실 OrtEmbedder를 라이브 `live_store`에 배선[모델 있으면 Ort, 없으면 Mock 폴백] + 실모델 의미 `#[ignore]` 테스트 + smoke_v10 + README/CLAUDE/index v0.10 bump). **단 task-50 전에 리팩토링 리뷰 먼저**(`docs/plans/refactoring-review-v9-snapshot.md` 작성: memory.rs cfg 다중 impl·recall 중복·embed/ann 구조·세션 길어 쌓인 cfg 점검). ② **web 프런트엔드**(Kimi UI 초안 `web/`, 데이터 계약→axum WS 배선). ③ **페르소나 합성 + 픽셀아트 캐릭터**(persona-ui §2~4, 싸이월드 미니미). ④ v0.7 boost.

## 2. 기술 스택

### 산출물 (tunaSalon 제품)

| 계층 | 기술 |
|------|------|
| 소스 언어 | Rust (edition 2021) |
| 앱 형태 | TUI(ratatui + crossterm) + headless 모드. 엔진 코어는 출력 sink와 분리 |
| 결정성 | 주입 ChaCha8Rng + 논리 ts + BTreeMap. 같은 seed → 바이트 동일 NDJSON |
| LLM 백엔드 | 기본은 LLM off(FakeBackend, 결정적). `--llm`이면 이종 백엔드 풀(v0.4). `Backend{Ollama,OpenAI}`: Ollama `/api/generate`(reqwest blocking) + OpenAI `/v1/chat/completions`(vLLM). **로컬 ollama 금지**(맥북 랙) → 기본 모델 `gemma4:31b-cloud`(cloud, 원격 프록시, 로컬 RAM 0), localhost+비`:cloud` 모델은 가드가 거부. 지인서버 vLLM `qwen3.6-35b-fast`(OpenAI, 동시성 1). 백엔드별 세마포어(cloud 3) + 라우팅 + 폴백 체인 |
| 임베딩 | BGE-M3 - v0.5(FlowMeter)부터 (초기엔 키워드/유사도 근사) |
| 비밀 | `OLLAMA_CLOUD_API_KEY`는 루트 `.env`(gitignored). `.env.example`만 커밋. 키는 https 원격 엔드포인트의 Authorization 헤더에만 |

> 화자 선택은 엔진이 결정적으로, 발화 내용 생성만 LLM이. 기본 실행은 LLM 없이 v0.1 골든과 바이트 동일(불변식).

### 개발 파이프라인 (tunaFlow, 메타)

| 역할 | 엔진 |
|------|------|
| 워크플로우 엔진 | tunaFlow (자체 파이프라인) |
| Architect (계획) | Claude (Opus) |
| Developer (구현) | Sonnet 서브에이전트 (Agent tool, model sonnet). **codex 비사용**(2026-06-02 사용자 지시) |
| Reviewer (검토) | Claude (Opus) - diff 직접 리뷰 + cargo test + 골든 검증 |

## 3. 빌드 / 테스트

Rust 프로젝트. `src/` 모듈: model, sink, hawkes, gate, rrf, utterance, driver, headless, tui, sweep, preset, runtime, ollama, openai, pool, semaphore, human(v0.5), live(v0.5 LiveSession), chat(v0.5 채팅 TUI), locale(v0.5 응답 언어 감지 $LANG, 기본 한국어, SALON_LANG override), flow(v0.6 FlowMeter 수렴/발산 토큰 근사), meta(v0.7 MetaController, 수렴→μ 식힘 mu_scale, SALON_META_GAIN), memory(v0.8~ friend engine: 참여 기반 사건/회상. v0.9 SQLite/FTS5 BM25+형태소+영속. v0.10 hybrid recall), tokenize_ko(v0.9 Lindera 형태소, friend-engine feature), embed(v0.10 Embedder/Mock/Ort BGE-M3, friend-engine-semantic), ann(v0.10 usearch HNSW, friend-engine-semantic). 회상 평가 하네스 `tests/recall_eval.rs`. **feature 계층**: `friend-engine`(v0.9: lindera+rusqlite) ⊂ `friend-engine-semantic`(v0.10: +ort+ndarray+tokenizers+usearch). 기본 빌드는 둘 다 off → lean·ML-free·골든.

```bash
cargo test                                       # 225 tests. friend-engine 234, friend-engine-semantic 260. 스모크 + recall_eval + smoke_chat + smoke_v9
cargo test --features "friend-engine-semantic coreml" -- --ignored ort_embed_basic --nocapture  # 실 BGE-M3 로드/embed 측정(모델 ~/.cache/tunaSalon/models/bge-m3/ 필요, 받아둠)
cargo run                                        # TUI (DebugMeter, 인터랙티브). q 종료, space 일시정지
cargo run -- --headless --seed 42 --ticks 200    # 결정적 NDJSON (틱당 한 줄)
cargo run -- --sweep                             # θ×k 격자 + room preset 비교
cargo run -- --room argument --fsm               # 케미 프리셋 + 같은 화자 2연속 금지
cargo run -- --llm                               # LLM 발화 생성(opt-in). 기본 cloud gemma4:31b-cloud. 로컬 모델은 가드가 거부(로컬 ollama 금지)
cargo run --example persona_collapse             # 같은 cloud 모델·다른 페르소나 3개 동시 출력 비교
cargo run --example mixed_bench                  # cloud + 지인서버(vLLM) 두 모델 한 방 동시 생성 + 1발화 지연 측정
cargo run -- --chat                              # v0.5 사람 참여 채팅방(인터랙티브 TUI, 실제 터미널 필요). 입력창에 타이핑→페르소나 반응
cargo run --example chat_demo                    # v0.5 라이브 루프 비인터랙티브 데모(스크립트된 사람 턴 + 전사, TTY 불요)
```

골든 베이스라인 5종은 `/tmp/salon_golden/`(로컬 dev 아티팩트, 레포 밖). seed/theta/ticks: s42_t040(120틱), s42_t065(80), s42_t078(120), s7_t065(80), s99_t065(80).

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

- **v0.5 (사람 참여 채팅방) - 완료**: HumanChannel(design §5: 큰 mark→전 λ 자극+화제 선점) + LiveSession(워커 스레드+mpsc로 ~1.6s 생성 논블로킹, in-flight 1개) + 채팅 TUI(persona-ui §5: 채팅 pane+게이지 사이드바+입력창) + `--chat`/`chat_demo`. 라이브 검증됨(사람 턴 후 페르소나가 한국어로 반응). headless는 dev 회귀 도구로 강등.
- **v0.6 (FlowMeter) - 완료**: 대화 수렴/발산을 토큰 중복으로 근사(`flow.rs`), record/채팅 사이드바 게이지("흐름 수렴 {bar}"), chat_demo 라이브 검증. **관찰만**(엔진 피드백 없음), content 게이팅으로 골든 보존. BGE-M3는 measure 인터페이스 유지한 채 이후 교체.
- **v0.7 (MetaController) - 완료**: 수렴↑이면 `mu_scale`(=base_rate 곱)을 낮춰 방을 식힘(`meta.rs`, driver/live 배선). 약한 게인(`SALON_META_GAIN`) + floor 0.4(고착/진동 방지). content 게이팅(flow None→mu_scale 1.0)으로 골든 보존. 채팅 사이드바 "식힘 ×" 표시. **주의**: chaos 데모룸은 발산만 해 식힘이 거의 안 걸림(정상) - 식힘 발동 보려면 수렴하는 대화 필요(threshold 0.5↑). 진동은 라이브 미터로 관찰·게인 튜닝.
- **v0.8 (friend engine 첫 증분) - 완료**: 참여 기반 인메모리 기억(`memory.rs`: 사건{방,ts,화자,내용}+참여) + 키워드 회상(토큰 중복, 라이브 경로만) + v0.3 회상 슬롯 주입 + SSOT 회상 평가 하네스(`tests/recall_eval.rs`). content 게이팅으로 골든 보존(driver/PersonaRuntime 경로 recall 미주입). BGE-M3/SQLite/망각은 심화 트랙.
- **v0.9 (friend engine 심화 Stage 0+1) - 완료**: 플랜 `docs/plans/salon-engine-v9.md`. Stage 0(Lindera 한국어 형태소 `tokenize_ko.rs`) + Stage 1(`:memory:`/파일 SQLite + FTS5 BM25 회상 `memory.rs`, 세션 넘는 영속 `~/.local/share/tunaSalon/memory.db`/`$SALON_MEMORY_DB`). seCall 검색코어 lift([[secall-search-core]]). 전부 `friend-engine` feature 뒤(기본 off→기본빌드 lean·rusqlite/lindera 미컴파일), 회상은 라이브 전용→골든 무손상. recall 시그니처 owned로 통일. task 43~46. **채팅 생동감**(별도): `chat_personas`(μ 0.70/0.62/0.55) + Pub 교차자극 + theta 0.60 + forbid_self_repeat + `/topic`(스모크 `smoke_chat`/`smoke_v9` 게이트). `SALON_CLOUD_ONLY`로 cloud x3.
- **v0.10 (friend engine Stage 2 = 의미검색) - 2a/b/c 완료, task-50 마감 미완**: 플랜 `docs/plans/salon-engine-v10.md`. 2a(task-47) Embedder/Mock/Ort BGE-M3 `embed.rs`(ORT 실측 viable 3.8s/29ms/2.3GB, `download-binaries`, [[ort-embedding-viable]]) / 2b(task-48) usearch HNSW `ann.rs` + `memory_vectors` BLOB + `vector_search` / 2c(task-49) hybrid RRF 회상(BM25 어휘+벡터 의미, 양 leg 참여격리, **v0.9 recall은 cfg로 보존**, RRF k=60). 전부 `friend-engine-semantic` feature 뒤, 임베더는 MockEmbedder(결정적 테스트). seCall 검색코어 lift([[secall-search-core]]).
- **다음(이후 트랙)**: ① **task-50 = v0.10 마감**(새 세션) - 실 `OrtEmbedder`를 라이브 `live_store`(memory.rs)에 배선(모델 있으면 Ort/없으면 Mock 폴백; **임베더는 DB당 일관해야** - Mock/Ort 혼용 금지, 바뀌면 ANN 재구축 필요) + 실모델 의미 `#[ignore]` 테스트(어휘≠의미 SSOT 회상) + smoke_v10 + README/CLAUDE/index v0.10 bump. **★ task-50 전에 리팩토링 리뷰 먼저**: `docs/plans/refactoring-review-v9-snapshot.md` 작성(memory.rs cfg 다중 impl·recall 중복·embed/ann 구조·긴 세션 누적 cfg 점검 후 정리). ② **web 프런트엔드**(`docs/plans/salon-web-frontend.md`: axum WebSocket, Kimi UI 초안 `web/`→데이터 계약 맞춰 배선). ③ **페르소나 합성+픽셀아트 캐릭터**(persona-ui §2~4, 싸이월드 미니미, "친구 만들기"=/invite). ④ v0.7 발산 boost. 인터랙티브 `--chat`은 실제 터미널 검증 권장.
- **사람 참여(design §5, v0.5 task-28~31에서 구현)**: 사람 = Hawkes 외부 이벤트(강도 상시 무한), 발화 시 **큰 mark**로 전 페르소나 λ 강자극 + 강도 일부 리셋 + **화제 선점**. §7 `HumanChannel`(입력층, **코어**), §8 "어느 틱에든 인터럽트". 데이터 모델 호환(`Event.speaker` 자유 문자열). 비결정이라 LLM-off 골든 경로 불침투(opt-in). 메모리 [[human-participation]].
- **v0.4 완료 사실(참고)**: 이종 백엔드 풀 = `Backend{Ollama,OpenAI}` + 백엔드별 세마포어(cloud `gemma4:31b-cloud` 3 / friend vLLM `qwen3.6-35b-fast` 1) + 페르소나 라우팅 + 폴백 체인. 동시성은 비교/벤치(`generate_batch`) 전용, 라이브 틱은 순차(인과). async 미도입. 라이브 1발화 지연 ~1.6s(cloud) → burst는 v0.4.x/v0.5 검토(인과성 충돌로 보류). Ollama Cloud Pro = GPU시간 정액 구독(토큰 종량 아님, 메모리 [[ollama-cloud-limits]]). 로컬 ollama 금지(맥북 랙), `:cloud`만 로컬 RAM 0.
- **미룸(설계 노트만, `docs/temp/`)**: friend engine(장기기억, 회상 슬롯 v0.3 인터페이스에 예약), KV 캐시 최적화(느림 측정 후), 페르소나 40조각 on-demand 조립. + task-24 보류분(outcome 분류/백오프/unhealthy-state).
- 세션 운영: 구현 위임은 **Sonnet 서브에이전트**(Agent tool, model sonnet), **codex 비사용**. Claude(Opus)가 스펙·리뷰·검증. 메모리 `~/.claude/projects/.../memory/`. README는 v0.4까지 반영됨.

---

> Auto-detected by tunaFlow. 내용을 검토하고 필요하면 수정하세요.