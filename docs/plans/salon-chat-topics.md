---
title: "Salon chat 기능: /topic 인터랙티브 주제 태그 (1~5개)"
type: plan
status: done
updated_at: 2026-06-03
status_note: "done 2026-06-03. 리뷰 통과(diff 정독+독립 재검증). 변경 chat.rs+live.rs+smoke_v5/6/7(render_chat 호출 &[]). 토픽은 워커 스냅샷에만 insert(0)(state.history/flow/recall 불변, INV-2 확인), 백엔드 시그니처 불변, 골든 5/5. /topic a,b,c 설정·/topic clear·최대5개. parse_topic_args/set_topics 테스트 7종."
owner: shared
summary: 채팅방에 주제 태그(1~5개)를 두고 페르소나가 그 주제로 얘기하게 한다. 주제 없으면 겉도는 스몰토크만 반복(2026-06-03 라이브 관찰). `/topic` 인터랙티브 명령으로 런타임 지정. 변경은 chat.rs+live.rs로 한정(백엔드 시그니처 불변): 토픽을 LiveSession에 저장하고 생성 워커로 보내는 history 스냅샷에만 "방 화제" 컨텍스트 줄 주입. state.history/FlowMeter/recall/골든 불침투.
notes_ref: ../../.. (memory [[topic-tags]])
---

# Salon chat - /topic 주제 태그

## 0. Context

라이브 관찰(2026-06-03): 주제가 없으면 페르소나들이 "그냥 쉬자/내려놓자"류 빈 위로를 무한 반복(겉도는 스몰토크). 실제 대화엔 "무엇에 대해"가 필요. 사용자 결정: **`/topic` 인터랙티브 명령**으로 방 주제 태그(1~5개) 지정. 메모리 [[topic-tags]]. (env/기본세트 방식은 거절, 수렴-기반 주제 회전은 BGE-M3 이후 2차.)

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | **골든 무관**: 주제는 라이브(`--chat`/LiveSession) 전용. driver/headless/`PersonaRuntime`는 토픽 미사용 → 백엔드 시그니처 불변 → 골든 5종 바이트 동일 |
| INV-2 | **불침투**: 토픽 컨텍스트 줄은 **생성 워커로 보내는 history 스냅샷에만** 주입. `state.history`(화면 렌더), FlowMeter(`flow()`), recall 쿼리(query 빌드)에는 안 들어감 |
| INV-3 | 백엔드(pool/ollama/openai) 시그니처·assemble 불변. 변경은 `chat.rs` + `live.rs`만 |
| INV-4 | 기존 테스트(렌더 4종, smoke, recall_eval) green 유지. render_chat 신규 인자는 기존 테스트 호출 갱신 |

## 2. Goals / Non-goals

### Goals
- (G1) `LiveSession`에 `topics: Vec<String>` 상태 + `set_topics(Vec<String>)`(trim/빈제거/최대 5개 cap) + `topics() -> &[String]`.
- (G2) 생성 디스패치 시 토픽 비어있지 않으면, 워커로 보내는 **history 스냅샷 앞에** 합성 컨텍스트 Event 1개 주입(예: speaker `"(방 화제)"`, content `"이 방의 화제: {a · b · c}. 막연한 메타토크 말고 이 주제로 구체적으로 얘기하세요."`). state.history엔 미주입.
- (G3) `chat.rs` 입력 루프: Enter 시 입력이 `/`로 시작하면 **명령으로 라우팅**(submit_human 안 함). `/topic a, b, c` → set_topics(쉼표 분리). `/topic`(빈) → 토픽 clear. 그 외 `/...` → 무시(입력만 비움).
- (G4) `render_chat`에 `topics: &[String]` 인자 추가 → 채팅 pane 제목에 표시(토픽 있으면 `"chat · 화제: a · b"`, 없으면 `"chat"`).

### Non-goals
- ❌ env/기본 주제 세트(거절됨). ❌ 수렴-기반 자동 주제 회전(BGE-M3 후 2차). ❌ 백엔드 프롬프트 슬롯 threading(시그니처 변경 회피 — 합성 history로 충분, 약하면 2차에 슬롯화). ❌ `/help` 등 다른 명령(이번 범위 밖, 토픽만).

## 3. Changed files

- `src/live.rs` - 수정. `LiveSession`에 `topics: Vec<String>` 필드(초기 빈), `set_topics`/`topics` 메서드. `with_store`/`new`에서 빈 Vec 초기화. `tick()`의 생성 디스패치에서 history 스냅샷 만든 직후(query/flow 계산 **이후**) 토픽 비어있지 않으면 합성 Event를 스냅샷 맨 앞에 prepend. **state.history·query·flow엔 미반영**.
- `src/chat.rs` - 수정. `render_chat(... , topics: &[String])` 인자 추가 + 채팅 pane 제목 동적화. `ChatApp::run` 입력 처리: Enter 분기에서 `input_buf.starts_with('/')`면 명령 핸들러(`/topic` 파싱 → session.set_topics/clear), 아니면 기존 submit_human. 렌더에 `self.session.topics()` 전달. 렌더 단위 테스트 4종 호출에 `&[]` 추가.

## 4. Change description

- **명령 파싱(chat.rs)**: `let line = input_buf.trim();` Enter 시 `if line.starts_with('/') { handle_command(line, &mut session); } else if !line.is_empty() { submit_human(...) }`. `handle_command`: `/topic`로 시작하면 나머지를 쉼표 분리·trim·빈제거·최대 5개로 `set_topics`. 나머지(빈)면 clear. 미인식 명령은 무시. 입력 버퍼는 항상 비움.
- **토픽 주입(live.rs)**: 기존 `tick()`은 query(최근 history content) + flow를 먼저 계산하고 recall을 구한다. 그 **다음** job에 넣을 history 스냅샷에만 합성 토픽 Event를 prepend. 즉 토픽은 모델 컨텍스트에만 보이고, 화면/측정/회상 쿼리엔 안 보인다(INV-2). 토픽 빈 Vec이면 주입 없음(현 동작과 동일).
- **표시(chat.rs)**: 채팅 pane 제목에 활성 토픽 표시 → 사용자가 `/topic` 후 즉시 헤더로 확인(별도 피드백 채널 불요).
- **골든**: driver/headless는 LiveSession을 안 쓰고 토픽도 없음. 백엔드 시그니처 불변. 골든 5종 재확인.

## 5. Verification

```bash
cargo build
cargo test
cargo build --features friend-engine
cargo test  --features friend-engine
# 골든 5종(기본 빌드, 명시적 순차 — set -- 금지)
cargo build
cargo run -q -- --headless --seed 42 --ticks 120 --theta 0.40 | diff - /tmp/salon_golden/s42_t040.ndjson && echo s42_t040 OK
cargo run -q -- --headless --seed 42 --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s42_t065.ndjson && echo s42_t065 OK
cargo run -q -- --headless --seed 42 --ticks 120 --theta 0.78 | diff - /tmp/salon_golden/s42_t078.ndjson && echo s42_t078 OK
cargo run -q -- --headless --seed 7   --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s7_t065.ndjson  && echo s7_t065 OK
cargo run -q -- --headless --seed 99  --ticks 80  --theta 0.65 | diff - /tmp/salon_golden/s99_t065.ndjson && echo s99_t065 OK
```

- 양쪽 빌드/테스트 green. 골든 5종 바이트 동일.
- 신규 테스트:
  - `LiveSession::set_topics` cap(6개 주면 5개로), trim/빈제거. `topics()` 반영.
  - `render_chat`가 토픽 있을 때 제목에 토픽 표시(TestBackend 버퍼에 화제 문자 존재), 빈이면 "chat".
  - (입력 파싱은 순수 함수로 분리해 단위 테스트 가능하면 좋음: `parse_topic_command("/topic a, b") -> vec!["a","b"]`).
- 수동 라이브: `SALON_CLOUD_ONLY=1 cargo run --features friend-engine -- --chat` → `/topic 주말 등산, AI 뉴스` 입력 → 헤더에 화제 표시 + 페르소나가 그 주제로 얘기.

## 6. Risks

| 위험 | 회피 |
|---|---|
| 토픽이 화면/FlowMeter/recall에 새어듦 | 스냅샷에만 주입(query/flow/recall 계산 이후). state.history 불변. INV-2 |
| 골든 깨짐 | 라이브 전용·백엔드 시그니처 불변. driver/headless 토픽 없음. 골든 재확인 |
| `/`로 시작하는 정상 발화 막힘 | 사용자가 `/`로 문장 시작할 일 드묾(채팅 명령 관례). 필요시 `//`로 escape는 2차 |
| 합성 speaker가 페르소나를 혼란 | content에 "막연한 메타토크 말고 이 주제로" 지시 포함. 약하면 2차에 프롬프트 슬롯으로 승격 |
| render_chat 인자 추가로 테스트 깨짐 | 렌더 4종 호출에 `&[]` 추가(스펙 명시) |

## 7. 산출물

- 이 문서. 한 줄: `--chat`에서 `/topic a, b, c`로 방 주제(최대 5개)를 지정하면, 그 주제 컨텍스트가 생성 프롬프트에만 주입돼 페르소나가 겉돌지 않고 그 화제로 얘기한다. 골든·측정·회상 불침투.
