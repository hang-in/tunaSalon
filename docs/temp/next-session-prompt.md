---
title: 다음 세션 첫 대화 복붙 프롬프트 (web 프런트엔드 트랙 P1 수직 슬라이스)
type: reference
status: active
updated_at: 2026-06-03
---

# 다음 세션 킥오프 프롬프트

아래 블록을 새 세션 첫 메시지로 복붙하세요. (v0.10 friend engine 의미검색까지 완료. 다음은 별개의 병렬 제품 트랙 = web 프런트엔드.)

---

```
tunaSalon 이어서 작업한다. 먼저 CLAUDE.md(핸드오프)와 docs/plans/salon-web-frontend.md, 그리고 최근 커밋(git log --oneline -15)을 읽어 현재 상태를 파악해.

현재: v0.1~v0.10 구현 완료(엔진 + friend engine 의미검색까지). default 226 / friend-engine 235 / friend-engine-semantic 263 tests, 골든 5/5 무손상. 다음은 엔진 버전 라인과 별개의 병렬 제품 트랙 = web 프런트엔드(채팅 UI를 브라우저로, "외부 사용자가 봤을 때 프로덕션 레벨 앱").

목표(salon-web-frontend.md): Rust 엔진은 재작성 0. axum WebSocket을 새 출력 sink로 얹어 엔진 이벤트(발화/강도/흐름/식힘/생각중)를 브라우저로 push + 사람 입력을 submit_human으로 받는다. 엔진은 blocking 유지하고 async는 채널 브리지로 격리(엔진 코어에 async 미도입). golden/headless/smoke 무손상(전부 `web` feature flag 뒤, 기본 빌드 무영향). 키는 서버에만(WASM-only 불가). TUI(chat.rs)는 디버그 sink로 동결.

착수 전에 결정 2가지를 먼저 나에게 물어라:
1) 프런트 형태: Kimi 초안(web/, React/Vite, 엔진상태 패널은 강하나 채팅영역 미완)을 살릴지 vs 플랜대로 정적 HTML/CSS/JS 1장(프레임워크 미도입)부터 갈지. 플랜 non-goal이 "프레임워크 미도입"이라 Kimi 초안과 방향이 갈린다.
2) 시작 범위: localhost 단일 세션부터(권장, 인증·멀티세션 없음, 프로토콜만 확장 여지) vs 처음부터 멀티세션 고려.

그다음 P1 수직 슬라이스를 task로 분해(salon-web-frontend-task-NN.md)하고 구현해라:
- axum WS 라우트 1개(`/ws` 업그레이드) + 정적 파일 서빙, `--web [--port N]` 플래그(opt-in), `web` feature flag(Cargo.toml). 기본 실행·`--chat`·`--headless`는 그대로.
- 엔진<->async 브리지: blocking LiveSession을 전용 스레드에서 구동 + tokio mpsc로 WS task와 양방향. 엔진 코어 무수정(LiveSession은 이미 워커 스레드 + mpsc로 논블로킹 생성 보유 → 재사용).
- 이벤트 직렬화 어댑터: 엔진 Event/intensities/FlowMetric/mu_scale을 web 프레임 스키마로(serde JSON). 서버->클라(utterance/intensities/flow/mu_scale/pending) + 클라->서버(human_message).
- 정적 1장: 채팅 로그(auto-scroll) + 사이드바 게이지(λ를 CSS 트랜지션으로 애니메이션, θ 마커, 흐름/식힘) + 입력창.
- 수직 슬라이스 한 바퀴(엔진 push -> 브라우저 렌더 -> 사람 입력 -> 엔진) 먼저 증명한 뒤 폴리시(P2~).

검증: 골든 5종 + 기본/friend-engine/friend-engine-semantic 테스트 전부 green 유지(web은 feature flag 뒤라 기본 빌드 무영향). WS 수직 슬라이스는 로컬 브라우저로 수동 확인.

구현은 Sonnet 서브에이전트(Agent tool, model sonnet)에 위임, Claude(Opus)가 스펙·리뷰·커밋. codex 비사용. 최종 답변은 한국어, em-dash 금지.
```

---

## 참고 (핸드오프 보강)

- **전송 = WebSocket 확정**(플랜 §5): 채팅앱은 양방향·빈번이라 단일 duplex가 자연스럽다. SSE+POST는 거절(두 채널 조율 + 인터럽트/타이핑 표현 어색). WS 추가 비용은 ping/pong keepalive + 재연결 직접 구현(localhost 단계선 무시 가능).
- **데이터 계약/UI 참고**: `docs/temp/salon-web-ui-kimi-prompt.md`(Kimi에 준 데이터 계약·UI 프롬프트), persona-ui §5(채팅 pane + 게이지 사이드바 + 입력창). Kimi 초안은 `web/`(React/Vite, 중앙 3D 큐브·채팅영역 미완).
- **키 보안**(INV-3): `OLLAMA_CLOUD_API_KEY`는 루트 `.env`(gitignored) + 서버 Authorization 헤더에만. WS 프레임/HTML/JS에 절대 노출 금지.
- **아키텍처 델타**: 신규 `src/web.rs`(axum 라우터 + WS<->LiveSession 브리지) + `web/` 정적 디렉터리. `Cargo.toml`에 `axum`+`tokio`(ws feature)를 `web` feature flag 뒤로.
- **리팩토링 주의(web sink 도입 시 재평가)**: refactoring-review-v9-snapshot.md 5-1/2-2 - `LiveSession`이 7가지 책임을 한 struct에 들고(엔진/디스패치/인풋/회상/화제/라벨/거시) `Arc<BackendPool>` 결합이 있다. web sink가 LiveSession을 그대로 가져가면 결합이 복제될 수 있으니, 브리지 어댑터를 얇게 두고 LiveSession 공개 API에 web 전용 타입을 새로 박지 말 것. 큰 분리는 변동이 실제로 생길 때.
- **골든 베이스라인**: 5종 `/tmp/salon_golden/`(레포 밖). 비교는 `cargo build` 후 명시적 순차 실행(zsh는 `$feat` 따옴표 변수 워드스플릿 안 됨 - feature 인자는 따로 호출).
