---
title: Salon 엔진 플랜 v0.8 - friend engine (장기기억, 최소 증분)
type: plan
status: done
priority: P1
updated_at: 2026-06-03
owner: shared
summary: v0.8 - 페르소나가 지난 대화를 기억하는 장기기억(friend engine) 첫 증분. 참여 기반 기억(사건{방,ts,화자,내용} + 참여{방,캐릭터}, 같은 방에 있던 캐릭터만 회상). 키워드 회상(토큰 중복, flow 재사용) + 인메모리 저장으로 시작(BGE-M3/seCall 포팅·SQLite는 이후). v0.3 회상 슬롯에 주입. 핵심 산출물 = 회상 평가 하네스(SSOT/distractor 심은 회상방 + 검색층 자동 채점 + 참여 격리). content 게이팅으로 골든 보존.
design_ref: ../reference/salon-engine-design.md
roadmap_ref: salon-engine-v1.md
notes_ref: ../temp/salon-memory-engine-idea.md
---

# Salon - 플랜 v0.8 (friend engine 장기기억)

## 0. Context

설계 노트 `docs/temp/salon-memory-engine-idea.md`·`salon-recall-eval-harness.md`·`salon-context-prompt.md`가 방향을 정해뒀다. 원래 단계 로드맵(v0.1~v0.7 엔진층)은 완료. v0.8은 그 위에 **장기기억**을 얹는 별도 트랙 - "늘 보던 페르소나가 지난 대화를 기억한 채 돌아오는 층".

목적은 친구 만들기가 아니라 **장기기억 자체를 구현·실험**하는 것이고, tunaSalon이 그 테스트베드다. L0 단기 공유 로그(완료) 위에 L1(SQLite 영속)을 건너뛰고 **L2 의미기억 실험을 일찍 시작**한다(노트 허용). 회상 슬롯은 v0.3 `assemble_user_prompt(recent, recall)`에 이미 예약됨(지금 None).

**첫 증분 범위(사용자 결정 2026-06-03)**: 키워드 회상(토큰 중복, `flow.rs` 토큰화 재사용) + 인메모리 저장 + 회상 평가 하네스. seCall 검색코어(BGE-M3/BM25/HNSW/형태소)·SQLite 영속(L1)·망각·주관적 저장·인물기억·판정 모델은 **이후**.

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | **content 게이팅**: FakeBackend(content 없음)면 저장할 사건도, 회상도 없음 → recall None → `assemble_user_prompt(recent, None)` → v0.1~v0.7 골든 바이트 동일 |
| INV-2 | **참여 기반 격리**: 캐릭터는 자신이 참여한 방의 사건만 회상. 없던 방의 사건 회상 금지(자동 채점으로 검증) |
| INV-3 | **결정성**: 저장·검색(토큰 중복)은 rng·네트워크·벽시계 없음. 같은 사건열+쿼리 → 같은 회상. 시간은 논리 ts(결정적) |
| INV-4 | 회상은 **검색만**(저장·망각 정책은 최소: 원발화 저장, 망각 없음). 엔진 결정(누가/언제)에 피드백 안 함(회상은 생성 내용에만 영향) |
| INV-5 | v0.7의 184 tests + 스모크 게이트 7종 유지(+ v0.8 게이트) |

## 2. Goals / Non-goals

### Goals
- (G1) **메모리 스토어 + 회상 코어**(`src/memory.rs`): 인메모리 사건{room, logical_ts, speaker, content} + 참여{room → 캐릭터 집합}. `recall(persona, query, k)` → 참여 방으로 좁힌 뒤 토큰 중복 상위 K. 순수·결정적.
- (G2) **회상 평가 하네스**: SSOT/distractor 심은 회상방 + 검색층 자동 채점(재현율/정확도 + 참여 격리). 헤드리스·결정적. **이 구조의 진짜 이점**(노트).
- (G3) **생성 배선**: 회상을 v0.3 회상 슬롯에 주입(Backend::generate에 recall 전달, assemble가 채움). pool/live가 사건 저장 + 생성 전 회상 검색. content 없으면 recall None(골든 보존).
- (G4) v0.8 스모크 게이트: 골든 보존 + 회상 결정성 + 참여 격리 + content 게이팅.

### Non-goals
- ❌ BGE-M3/seCall 검색코어 포팅(BM25/HNSW/형태소) - 이후. 키워드 근사로 시작(measure류 인터페이스 유지).
- ❌ SQLite 영속(L1) - 인메모리로 시작(노트: L2 일찍 시작 가능).
- ❌ 벽시계·주입형 시계 + "3일 전" 정밀 상대표현 - 논리 ts 기반 "지난 대화에서"로 시작.
- ❌ 망각 정책, 주관적(캐릭터별) 저장 정책, 인물기억(방 가로지르는 인상), 발화층 judge 채점 - 모두 이후.

## 3. 데이터 모델 / 아키텍처 델타

| 구조 | 변경 |
|------|------|
| `MemoryEvent`(신규) | `{ room: String, ts: u64(논리), speaker: PersonaId, content: String }` |
| `MemoryStore`(신규 `src/memory.rs`) | `events: Vec<MemoryEvent>` + `participation: BTreeMap<room, BTreeSet<PersonaId>>`. `record(event)` / `join(room, persona)` / `recall(persona, query, k) -> Vec<&MemoryEvent>`(참여 방 필터 + 토큰 중복 상위 K) / `format_recall(events) -> String`("지난 대화에서: ...") |
| `Backend::generate` / `PersonaRuntime` | `recall: Option<&str>` 전달 경로(assemble 슬롯에 연결). pool/live가 검색해 주입 |
| `pool.rs` / `live.rs` / `driver.rs` | 사건을 MemoryStore에 record(content 있을 때만) + 생성 전 recall 검색. content 없으면 recall None → 골든 보존 |
| 평가 하네스(테스트/예제) | 회상방 시나리오(SSOT+distractor+참여) + 검색층 채점(recall@K, precision, 격리) |

## 4. Subtasks

| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| 39 | 메모리 스토어 + 회상 코어 | `memory.rs`: 사건/참여 인메모리, recall(참여 필터 + 토큰 중복 상위 K, flow 토큰화 재사용), format_recall. 순수·결정적. 단위 테스트(참여 격리, 토큰 회상, 빈→없음) | 낮음(순수) | v0.7 |
| 40 | 회상 평가 하네스 | 회상방(SSOT+distractor+참여 3인) 헤드리스, 검색층 자동 채점(재현율/정확도/참여 격리). 결정적. 노트의 핵심 이점 | 중(채점 임계·시나리오 설계) | 39 |
| 41 | 생성 배선(회상 주입) | recall을 Backend::generate→assemble 슬롯에 연결. pool/live가 사건 record + 생성 전 recall. **content 없으면 None→골든 보존**. --chat 라이브 | **중~높음**(생성 경로 + 골든 보존, 시그니처 churn) | 39 |
| 42 | v0.8 게이트 + 마감 | smoke_v8(골든 보존, 회상 결정성, 참여 격리, content 게이팅) + (선택) 채팅에 회상 표시. chat_demo로 라이브 관찰 | 낮음~중 | 40, 41 |

Phase A(39 코어) → B(40 평가 하네스, 41 배선) → C(42 게이트). 완료: task-42 + 골든 보존.

## 5. v0.8 완료 기준

- 기본 `cargo run`(LLM off)·headless 골든 5종 바이트 동일(content 없음 → 사건 없음 → recall None).
- `--chat`/`--llm`에서 페르소나가 **지난 대화를 회상**해 발화에 반영(회상 슬롯 주입). 같은 방 참여 캐릭터만.
- 회상 평가 하네스: SSOT 재현율·정확도·참여 격리가 검색층에서 자동 채점되고 결정적.
- 회상 계산 결정적(같은 사건열+쿼리 → 같은 결과).

## 6. 위험과 대응

| 위험 | 대응 |
|------|------|
| 회상 주입으로 골든 깨짐 | content 게이팅(FakeBackend 사건 0 → recall None → assemble(recent,None) 동일). 골든 5종 재확인(필수) |
| 참여 격리 실패(없던 방 회상) | recall이 참여 방으로 먼저 좁힘. 평가 하네스 격리 케이스로 자동 검증(INV-2) |
| 토큰 회상 부정확(한국어) | v0.8은 근사가 목표. flow 토큰화 재사용. 정밀도는 BGE-M3(이후). 평가 하네스가 회귀 보호 |
| 생성 경로 시그니처 churn | recall: Option<&str>을 generate 경로에 추가, 기존 호출 None. 빌드 확인 |
| 회상이 엔진 결정에 새어듦 | 회상은 생성 내용(프롬프트)에만. gate/rrf/hawkes/cooling 입력에 불사용(INV-4) |
| 비결정 유입 | 토큰 중복 + 논리 ts(결정적). 벽시계 없음. 비결정은 LLM content뿐 |

## 7. 산출물

- 이 문서(PLAN v0.8). 구현 시 §4를 `salon-engine-v8-task-39..42.md`로 분해.
- v0.8 한 줄: 페르소나가 자기가 있던 방의 지난 대화를 키워드로 회상해 발화에 끼워넣고, 그 회상 품질을 SSOT 심은 회상방으로 자동 채점한다. 임베딩·영속·망각은 이후. content 없으면 아무 일 없음(골든 보존).
