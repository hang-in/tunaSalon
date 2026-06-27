---
type: reference
status: snapshot
updated_at: 2026-06-27
title: tunaSalon 리팩토링 검토 — web 트랙 스냅샷
supersedes: refactoring-review-v9-snapshot.md
---

# tunaSalon 리팩토링 검토 — web 트랙 스냅샷 (2026-06-27)

> 검토 대상: `/Users/d9ng/privateProject/tunaSalon` (Rust 2021). v9 스냅샷(2026-06-03) 이후
> web 트랙 + 토론(debate)으로 `live.rs` 1059→2322, `web.rs` 0→2105, `memory.rs` 1494→1818.
> 검토 방식: god-file 3개(live/web/memory)를 general-purpose 서브에이전트 3개 병렬 진단 → Opus 종합.
> 검토 기준: `../prompts/refactoring-review-prompt.md` + dsp_cad_gcs 규율([[refactoring-discipline]]).
> **결정성(golden byte-identical) 최상위 제약.** baseline: 237 tests pass, headless byte-identical 확인됨(2026-06-27).

---

## 0. 실측 (2026-06-27)

god-file: `live.rs` 2322 · `web.rs` 2105 · `memory.rs` 1818 · `pool.rs` 1222 · `persona_kit.rs` 1213 · `main.rs` 1200.
(텍스트 producer 로직은 이미 `debate/` 모듈로 추출 완료 — 비대화 주범은 producer 가 아니라 LiveSession 책임 누적 + web 신규 표면.)

---

## 1. 핵심 결론 3줄

1. **실제 R1 안전 추출 가능 영역**: memory 순수 SQL 헬퍼(verbatim 38줄+ 중복), live flow-window(3중 인라인), web `build_state`(90줄 read-only)+From impl+presence JSON(4곳). 전부 골든/회귀 무영향.
2. **web.rs `serve()` 단일방 경로(~235줄)는 호출자 0 = 죽은 코드** + `serve_multi` 구조 중복. 제거 이익 크나 파괴적 → 승인 후.
3. **R2/R3 보류**: per-tick `decide_one_tick` 통합(결정성·rng), LiveSession 12책임군 분리, hybrid recall leg 통합(결정성), `live_store` 부수효과 좁히기(pub 제약), memory trait(분기 배타라 불필요).

---

## 2. 검증된 Finding (코드 라인 인용 기준)

### live.rs
| id | 설명 | 근거 | valid | 회귀 | 이익 | 단계 |
|---|---|---|---|---|---|---|
| L1 | flow-window 계산 3중 중복 (driver.rs 35-41/374-379 + live.rs 860-871): `history→content필터→saturating_sub(FLOW_WINDOW)→flow::measure` | 라인 인용 | ✓ | 낮(골든검증) | 중 | **R1** |
| L2 | `liveliness`(896)/`speaker_label_for_generation`(354)/`extract_conclusion_section`(1242, 기본빌드 dead_code warning) free fn 이동 | warning | ✓ | 낮 | 소 | **R1** |
| L3 | driver↔live per-tick 부분중복(스텝1-8 거의 동일, suppress는 live 인라인 vs driver free fn) | 461-747↔31-159 | ✓ | **높(rng소비순서)** | 중(미래) | R3 보류 |
| L4 | LiveSession 12책임군(엔진/입력/디스패치/회상/화제/라벨/거시 + 신규: phase·리포트·동적persona·복원·사람라우팅·취소·활기·정체성) | 30필드 | ✓ | 중 | 중 | R2~R3 |
| L5 | `restore_history`/`set_report`/`set_topics` 순서 타입갭(report=종료 추론) | 1012-1029 | ✓ | — | — | **버그영역(별도)** |

### web.rs
| id | 설명 | 근거 | valid | 회귀 | 이익 | 단계 |
|---|---|---|---|---|---|---|
| W1 | `serve()` 단일방 ~235줄 호출자 0(죽은 코드) + `serve_multi` 중복 | grep 0건 | ✓ | 낮(미사용) | 큼 | R1(삭제=파괴적·승인) |
| W2 | `build_state`(414-501) 90줄 closure, session read-only → 추출 | read-only | ✓ | 낮 | 중 | **R1** |
| W3 | ReportRecord→ReportDto 2곳(396/936) + presence JSON raw 4곳(1265/1312/1595/1648) 중복 | 라인 | ✓ | 낮 | 소 | **R1** |
| W4 | 화자명/센티넬("(진행)"/"Moderator"/"토론을 시작합니다.") live↔web↔chat 분산(표시 의미론 누수) | 450-485↔354-372 | ✓ | 중 | 중 | R2 |
| W5 | 인증 전무(외부 Caddy basic_auth 의존, 앱 내부 미강제) + DELETE 무인증 파괴 | 1390-1425 | ✓ | — | — | 보안 별도 트랙 |
| W6 | 클라 presence 프레임 신뢰 → `client_count` 덮어쓰기 가능(clients:0 → 방 정지) | 294/635-660 | 확인필요 | — | — | 확인 필요 |

### memory.rs
| id | 설명 | 근거 | valid | 회귀 | 이익 | 단계 |
|---|---|---|---|---|---|---|
| M1 | hybrid recall BM25 leg(894-945) ↔ BM25-only recall(752-848) verbatim 38줄+ 중복 | 라인 대조 | ✓ | 높(rank/결정성) | 큼 | R2(leg 통합) |
| M2 | `fts_or_match`(785/899) + `sql_placeholders`(798/908/957-968) 순수 문자열, 2~4곳 중복 | 라인 | ✓ | **낮(순수·SQL텍스트동일)** | 중 | **R1** |
| M3 | vec/sqlite/semantic trait 추상화 | — | trait✗ | — | — | 건드리지 말 것(분기 배타) |
| M4 | `live_store` 글로벌 디스크 부수효과 미해소(main.rs 3 + web.rs 1 호출, pub 필수라 좁히기 제약) | 549/1168 | ✓ | 중 | 중 | R2(대안 필요) |
| M5 | `record` FTS insert 실패 `let _` silent → BM25 회상 누락 가능 | 664 | 확인필요 | — | — | 확인/수정 후보 |

---

## 3. 착수 권장순 (R1 — 한 커밋 한 단위, 검증/commit 분리)

1. **memory M2** — `fts_or_match` + `sql_placeholders` 순수 헬퍼 추출. 엔진/골든 무관, friend-engine 테스트로 검증. (가장 순수)
2. **live L1** — flow-window 계산을 `flow.rs` 헬퍼로. headless 골든 byte-identical 로 검증.
3. **web W2+W3** — `build_state` free fn 추출 + `From<ReportRecord> for ReportDto` + presence JSON 헬퍼. read-only.
4. **live L2** — `extract_conclusion_section` 등 free fn 이동(dead_code warning 해소).

각 단계: `cargo test` (+friend-engine/web) → headless 골든 byte 비교 → commit.

---

## 4. 지금 건드리지 말 것

| 항목 | 이유 |
|---|---|
| per-tick `decide_one_tick` 통합(L3) | rng 소비 순서가 골든 불변식. 별도 세션·골든 철저. R3 |
| LiveSession 일괄 책임 분리(L4) | 12책임군 한 번에 쪼개면 회귀가 여러 모듈로. 변동 일어날 때 1~2개씩 |
| memory trait(M3) | vec/sqlite 런타임 배타 컴파일 — trait이 시그니처 동기 문제를 못 풀고 진짜 중복(M1)도 못 건드림 |
| hybrid recall leg 통합(M1) | rank/결정성 직접 영향. R2, 결정성 테스트 동반 |
| `serve()` 삭제(W1) | 파괴적 → 명시 승인 후 |

---

## 5. 확인 필요

| 항목 | 이유 |
|---|---|
| **복원 시 종료 토론 재발화 버그(L5)** | c7cdbc3(set_topics=report SSOT + run_engine 게이팅) 후에도 라이브 재발화 보고됨(2026-06-27). restart 실제 수행 여부 / redis-bus 복원 경로 / 오염된 기존 방 가능성 미확인. **리팩토링 후 재조사** |
| W6 클라 presence 신뢰 | 프런트가 실제로 presence 프레임을 보내는지 |
| M5 FTS insert 실패 silent | 사건이 memories엔 있고 fts엔 없어 회상 누락 가능 경로 |

---

## 6. 검증 로그

- 진단: general-purpose ×3 병렬(live/web/memory), 2026-06-27. Opus 종합.
- baseline: `cargo test` 237 pass / 3 ignored. headless byte-identical(s42 120틱, s99 80틱) 확인.
- 다음 스냅샷 권장: 페르소나 합성 1차 후, 또는 R2(per-tick/책임분리) 착수 직전.
