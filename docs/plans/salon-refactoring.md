---
type: handoff
status: in_progress
updated_at: 2026-06-27
---

# tunaSalon 리팩토링 핸드오프 (다음 세션)

> 목적: 다음 세션은 **tunaSalon 리팩토링**. 방법론은 **dsp_cad_gcs의 코드작성/리팩토링 문서**를
> 기준으로 한다(사용자 지시). 이 문서는 ① 직전 세션 상태 ② dsp_cad_gcs에서 가져온 리팩토링 규율
> ③ tunaSalon 리팩토링 후보 ④ 진행 방법 ⑤ 첫 프롬프트 를 담는다.

---

## 0. 직전 세션 상태 (2026-06-27, 배포 + 토론 리포트 기능)

**배포 - `salon.d9ng.co.kr` 라이브** (상세 `docs/plans/salon-web-deploy.md`, 메모리 [[homelab-deploy]])
- 홈랩 Caddy(external geo_kr_only+basic_auth / internal) + WireGuard → **n100(192.168.1.121) bare 바이너리 + systemd `tunasalon.service`**. 로컬 ollama(:cloud). 로그인 basic_auth(user=`d9ng`).
- 배포 스크립트 `~/deploy-salon.sh`(n100): pull+프런트빌드+백엔드릴리즈빌드+restart. **백엔드 변경 시만 restart 필요**(systemd 메모리 프로세스), 프런트만이면 새로고침.
- CI 자동배포는 안 함(GitHub public repo → self-hosted runner 보안 위험. Gitea 미러는 보류). 수동 스크립트 채택.

**기능 - 토론 리포트 히스토리 + 고급 모델** (커밋 `8535d52`~`374394f`)
- `room_reports` 테이블(SSOT, seq 누적) + `summarize_debate(past_conclusions)`(직전 ≤2 결론 참조) + State 프레임 `reports[]`(재진입 복원) + 사이드바 "지난 리포트" 목록/모달.
- 고급 cloud 모델 2종 추가: `minimax-m3:cloud`, `deepseek-v4-pro:cloud`(`CLOUD_MODELS`/`MODEL_OPTIONS`).
- 버그픽스: 언어(systemd `SALON_LANG=ko_KR.UTF-8`) / 중복 리포트 카드(state.reports[] 단일화) / 카드 정렬(transcript 끝) / **재진입 시 종료 토론 재개**(복원 순서 `set_report→set_topics→restore_history` - set_topics가 phase를 Opening으로 덮던 것, 회귀 테스트 2개 추가).

> ✅ [2026-06-27 해결] `374394f`는 라이브(web) 실패(main.rs factory 만 고치고 web `run_engine`의 `set_topics` 재호출을 놓침). `c7cdbc3`로 재수정: `set_topics`가 `report`(종료 SSOT) 있으면 Concluded 생성 + `run_engine`이 복원방(history有)엔 `set_topics` 스킵 + 단위테스트 2개. **restart 후 라이브 재발화 멈춤 확인** - 직전 "라이브 여전"은 n100 restart 누락(빌드만 됨)이 원인이었음([[debate-restore-rerun-bug]]). 추가로 로비 [결론 남] 배지 서버 권위화(`647a6a6`) + 깜빡임/정렬 회귀 재수정(`29883a3`, `serverConcluded` 별도 state 로 분리 - recentRooms 직접 갱신이 lobbyRooms 재계산 유발했던 것).
>
> [2026-06-27 진행] 리팩토링 R1 3개 done(스냅샷 `docs/reference/refactoring-review-2026-06-27-web.md`, push됨): flow `measure_recent`(67770f9) / memory SQL헬퍼 `fts_or_match`·`sql_placeholders`(b6a7d6c) / web `build_state` 모듈추출+`From<ReportRecord>`(246490d). 전부 골든 byte-identical, 동작 무변경. 남은 R1(L2, 이익 작음)·R2/R3는 스냅샷 참조.

---

## 1. 다음 세션 목표

tunaSalon 코드 리팩토링. **dsp_cad_gcs 규율(§2)을 적용**해 ①진단 → ②리뷰 리포트 → ③선별 실행. **빅뱅 금지·점진·결정성(golden) 보존**이 최상위 제약.

### 출처 문서 (다음 세션이 읽을 것)
- dsp_cad_gcs 규율(SSOT): `/Users/d9ng/workProject/dsp_cad_gcs/docs/reference/developmentConventions.md`, `docs/report/refactoring_review_2026-06-{02,07}.md`, `docs/plans/refactorBoundariesPlan_2026-05-26.md`, `docs/plans/systemReviewRefactor_2026-06-10.md`, `docs/reference/architectureDiagnosis_2026-06-01.md`. (skill: `dsp-cad:cad-conventions`)
- tunaSalon 기존 리팩토링 노트: `docs/reference/refactoring-review-v9-snapshot.md`(보류 후보), `docs/prompts/refactoring-review-prompt.md`.

---

## 2. dsp_cad_gcs에서 가져온 리팩토링 규율 (전이 가능 핵심)

**바로 채택할 Top 7**
1. **순수함수 추출 + 단위테스트** - 분기/계산을 순수함수로 격리 후 테스트. 테스트 없는 거동 변경 금지.
2. **SSOT 단일화** - 데이터 모델 이중화 금지. primary 1개 + 파생은 어댑터. (tunaSalon: `room_reports` SSOT / room_meta.report는 로비 배지 파생 - 이미 일부 적용.)
3. **Verify/Commit 분리 + 결정성 보존** - 검증(build→test→golden diff)과 commit/push를 별도 단계로. golden 5개 seed byte-identical 매번 확인.
4. **대상 우선순위 P0/P1/P2** - P0=침묵실패·테스트0순수추출·타입안전, P1=god-file 분해·중복·층 역참조(선택), P2=건드릴 때 타입 격상(트리거).
5. **Finding 분류: valid + 회귀 + 이익** - 발견 ≠ 실행. 코드/테스트로 valid 증명한 것만, 회귀 낮고 이익 명확한 것만 착수. (dsp 사례: 높음 19개 중 valid 7, 실행 3.)
6. **선제적 대규모 재설계 금지(YAGNI)** - "나중에 쓸 것 같아서"·"더 깔끔해서" 금지. 요청 범위·local 개선만.
7. **테스트 0 영역은 건드릴 때 그 부분부터** - 5줄도 테스트 동반. UI/부수효과 코드는 무리하게 순수화하지 말고 "skipped" 존중.

**방법론**
- 리팩토링 대상 식별 기준: god-file(500줄+, 책임 혼재), 중복(2회+), 계층 역참조(sink→core 의존 등), 침묵 실패, 타입 갭.
- 추출 안전 조건: 순수함수만 / 시그니처 유지 / import 순환 금지 / 한 커밋 한 단위 / 테스트 동반. **부수효과(mpsc, state mutation, 렌더 타이밍) 얽힌 코드는 추출 금지**.
- 단계화: R1(안전·subagent 위임) → R2(중간) → R3(상태 owner 재설계·main만). 선행조건 지킴(R1 완료 후 R2).
- 위임 명세 필수: Constraints(건드릴 것/말 것) · 시그니처 · 의존성 · Acceptance · 검증.

**리뷰 리포트 구조**(C2): ①핵심 결론 3줄 ②검증된 finding 표(id|영역|설명|런타임 재현|권장) ③착수 권장순(1·2·3 + 범위) ④지금 건드리지 말 것 ⑤확인 필요 ⑥검증 로그.

**Anti-patterns**: 검증-커밋 한 배치 / `test | tail` exit 오독 / "거동 변경 없다"며 무관 정리(import 정렬·포맷·이름변경) / 부수효과 코드 "순수화" 허위 주장 / "반증 없으니 맞다"(코드 인용으로 증명).

---

## 3. tunaSalon 리팩토링 후보 (다음 세션이 valid 검증할 것 - 미확정)

> 아래는 **후보**다. dsp 규율 C1대로 코드로 valid·회귀·이익을 검증한 뒤 선별한다(전부 실행 아님).

**god-file (라인수 실측 2026-06-27)**: `live.rs` 2322 · `web.rs` 2105 · `memory.rs` 1818 · `pool.rs` 1222 · `persona_kit.rs` 1213 · `main.rs` 1200. 테스트는 대부분 보유(live 29 / web 23 / pool 31 / memory 28) → "test 0" 문제는 작음. **god-file 책임 분해가 주 후보**.

**CLAUDE.md 보류분 + v9 스냅샷**:
- `driver`/`live` per-tick 알고리즘 통합 = `decide_one_tick` 추출(2-1). **중간 리스크·결정성 민감** → R2/R3, 별도 세션·골든 철저.
- `live_store` side-effect 격리(2-2). 단 main.rs가 lib free fn 호출 → `pub(crate)` 불가, 대안 필요. R3.
- v0.10에서 안전 적용한 것: FLOW_WINDOW 상수 단일화(1건). → 같은 결로 작은 R1부터.

**관찰(미검증, 다음 세션 valid 판단)**: live.rs(2322)·web.rs(2105)는 LiveSession 워커/멀티룸 엔진 루프가 한 파일에 혼재 - 책임 분해 후보지만 mpsc/동시성 얽힘 = 추출 위험(R3). 우선 **순수 계산부(리포트 결론 추출, 타임라인 병합, 게이지 계산 등)부터 추출(R1)**.

---

## 4. 진행 방법 (이 순서로)
1. dsp 규율 문서(§1 출처) + tunaSalon 기존 노트 정독.
2. 직전 픽스 라이브 확인(재진입 토론).
3. **진단**: god-file/중복/층역참조/타입갭/침묵실패 식별 → P0/P1/P2 분류.
4. **리뷰 리포트**(C2 구조)로 후보 정리 + valid/회귀/이익 라벨. `docs/reports/` 또는 `docs/reference/`에.
5. **R1(안전)부터** 한 단위씩: 순수함수 추출 + 테스트 + golden 무손상 확인 → commit(검증/commit 분리).
6. R2/R3는 별도로, 결정성 검증 철저. 구현 위임은 **Sonnet 서브에이전트**(명세 5요소), Opus가 스펙·리뷰.

**검증 명령(매 단계)**:
```bash
cargo build && echo BUILD_OK
cargo test && echo TEST_OK
# golden: cargo build 후 명시적 순차 실행(for-loop 안 cargo run 금지 - 첫 실행 재빌드로 거짓 회귀)
cargo run -- --headless --seed 42 --ticks 120 > /tmp/out.ndjson && diff /tmp/salon_golden/s42_t040.ndjson /tmp/out.ndjson && echo GOLDEN_OK
```

---

## 5. 다음 세션 첫 프롬프트 (복붙)

> tunaSalon 이어가기. `docs/plans/salon-refactoring.md` + 스냅샷 `docs/reference/refactoring-review-2026-06-27-web.md` 읽었어.
> 2026-06-27 done(전부 라이브 배포·확인): 종료토론 재발화(c7cdbc3) + 로비 배지 서버권위(647a6a6)+깜빡임 재수정(29883a3) + R1 3개(flow/memory/web) + R2 W4(센티넬·persona_display_name, 5eae34c)·L4(리포트 debate::report 분리, a06b55e). 다음:
> 2026-06-27 R2 추가 done: M1 안전부(participated_rooms/row_to_memory_event, 5a50b2a) · LiveSession 라벨(build_speaker_labels, 8800283).
> **R3 - 2026-06-27 전부 done(Opus 직접 구현·검증, golden/recall fixture byte-identical):**
> ① per-tick 결정적 코어 추출(5ce17eb) - rng-free 앞부분만 driver 헬퍼로 공유: `advance_intensities`(μ갱신→감쇠→combined) / `filter_self_repeat` / `suppress_chosen` 재사용. **decide_one_tick 전체 통합은 보류** - live 화자선택(forced/summary/closing)이 rng 소비를 분기별로 건너뛰어 골든 불변식 위험. 골든 5/5 byte-identical.
> ② M1 BM25 leg 통합(b397b78) - BM25-only를 1단계 full-row → 2단계(id-leg+fetch)로 재구조화해 hybrid와 통일: `bm25_leg_ids` / `fetch_events_by_ids` 공유. bm25 정렬·tie-break 동일 → recall fixture(scenario 6케이스×양 빌드) byte-identical 검증.
> ③ LiveSession 디스패치 캡슐화(e8880d9) - `GenerationWorker`(job_tx/result_rx/worker + spawn/dispatch/try_recv/shutdown)로 transport 분리. **입력(human_focus) 분리는 보류** - last_human_msg/human_focus/forced_next_speaker가 submit_human·tick 전반에 얽혀(field 이동 시 big-bang) 가치 대비 위험 과다. transport 테스트 통과 + 골든 무손상.
> M4(live_store 가드)는 보류 - 안전 수정 없음(pub 필수 + cfg(not(test)) web 테스트 충돌), 스냅샷 참조.
>
> **다음 후보(미착수)**: ③의 입력 모듈 분리(human_focus) - struct 분해 빅뱅이라 가치 입증 후. god-file 책임 분해(live.rs/web.rs/memory.rs, §3). live_store side-effect 격리(§3, pub 대안 필요).

> 메모리: [[refactoring-discipline]] [[homelab-deploy]] [[delegate-sonnet-not-codex]] [[mac-build-env]]
