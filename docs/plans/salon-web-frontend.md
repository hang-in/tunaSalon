---
title: Salon 플랜 - web 프런트엔드 트랙 (TUI는 디버그용으로 강등, 엔진 버전 라인과 별개의 병렬 제품 트랙)
type: plan
status: planned
priority: P1
updated_at: 2026-06-03
owner: shared
summary: "외부 사용자가 봤을 때 제대로 된(프로덕션 레벨) 앱"을 목표로, 채팅 UI를 web으로 옮긴다. Rust 엔진은 그대로 두고(재작성 0), axum WebSocket 서버를 새 출력 sink로 얹어 엔진 이벤트를 브라우저로 push + 사람 입력을 받는다. TUI(chat.rs)는 디버그 sink로 강등(유지). golden/headless/smoke는 무손상(엔진 회귀 하네스 그대로). WASM-only는 불가(LLM 키가 서버에 있어야 함). 전송은 WebSocket(채팅앱 정석). 프런트는 Kimi 초안(web/, Vite+React+TS+Tailwind+shadcn/ui+three.js)을 채택해 배선한다(2026-06-03 결정). 서버는 0.0.0.0 바인딩으로 LAN 접속 허용, 외부는 사용자가 Cloudflare 터널로 처리.
design_ref: ../reference/salon-engine-design.md
ui_ref: ../temp/salon-persona-ui.md
roadmap_ref: salon-engine-v1.md
---

# Salon - web 프런트엔드 트랙 (병렬 제품 트랙)

> 번호 주의: 엔진 버전 라인(v0.1~v0.8, 다음 v0.9=friend engine 심화)과 별개의 **병렬 제품 트랙**이다. 한때 "v0.9"로 적었으나 v0.9는 friend engine 심화에 배정됨(2026-06-03). 이 트랙은 엔진 버전 번호를 점유하지 않는다.

> 상태: **planned (착수 전, 결정만 기록)**. 사용자 지시(2026-06-03): "플랜 문서로 만들어 두고 하던 작업 이어가자" - 즉 이 트랙은 **파킹**해 두고 다른 작업을 먼저 진행한다. 이 문서는 web 전환 결정과 아키텍처를 잃지 않으려는 기록이다.

> **갱신(2026-06-03, v0.10 완료 후 착수 결정 확정)**:
> - (a) **프런트는 Kimi 초안(`web/`, Vite + React + TS + Tailwind + shadcn/ui + three.js) 채택**. 아래 §2의 "프레임워크 미도입" non-goal은 **폐기**한다(레거시). axum은 Vite 빌드 산출물(`web/dist`)을 정적 서빙하고, 개발 중에는 Vite dev server + `/ws` 프록시를 쓴다.
> - (b) **서버는 `0.0.0.0:PORT` 바인딩으로 내부 네트워크(공유기 192.168.1.X) 접속 허용**. 외부 노출은 사용자가 **Cloudflare 터널**로 처리한다(서버는 터널/포트포워딩에 비관여). 인증은 여전히 범위 밖(신뢰된 홈 LAN 가정).

## 0. Context / 동기

목표가 바뀌었다(2026-06-03 사용자): **"외부 사용자가 봤을 때 제대로 만든 TUI구나(프로덕션 레벨)"**. 처음엔 라이브러리 도입(ratkit)으로 접근했으나, 핵심 통찰은 *라이브러리가 프로덕션 느낌을 주는 게 아니라* (위젯의 올바른 사용 + 견고한 입력 + 일관된 상태 처리)가 준다는 것. 그리고 그 목표라면 매체 자체를 **web으로 옮기는 게 더 우위**라는 판단에 도달.

이 제품(사람이 LLM 페르소나들과 스몰토크하는 채팅방)에 web이 특히 맞는 이유:
1. **엔진이 자기 리듬으로 push**한다(Hawkes 타이밍). server-push 전송과 정확히 맞고, 강도 차오름·θ 근접·침묵에 안달하는 **"생동감"이 애니메이션 DOM에서 터미널 셀보다 훨씬 잘 보인다**(엔진의 차별점을 더 잘 전시).
2. **비밀 키(`OLLAMA_CLOUD_API_KEY`)는 서버에 있어야 한다**(하드 제약). → 어차피 서버가 필요 → 클라이언트를 브라우저로. (그래서 **WASM-only는 불가**: 브라우저에서 LLM 키를 못 들고, 결국 프록시 서버가 필요해 의미 없음.)
3. **공유 가능**: "Rust 레포 clone 후 터미널 실행"보다 URL 하나가 외부 사용자에겐 비교 불가한 진입성.

핵심 프레이밍: **재작성이 아니라 엔진 위에 새 sink를 얹는 것**. TUI가 그랬듯 web도 또 하나의 출력 sink일 뿐, 엔진 코어와 dev 회귀 하네스는 손대지 않는다.

## 1. Invariants

| ID | 내용 |
|----|------|
| INV-1 | **엔진 재작성 0**: model/hawkes/gate/rrf/flow/meta/memory/pool/backends 그대로 재사용. web은 LiveSession을 구동하고 그 이벤트를 직렬화하는 어댑터일 뿐 |
| INV-2 | **golden/headless/smoke 무손상**: web은 새 sink. driver/headless 경로 불침투 → v0.1~v0.8 골든 바이트 동일 유지 |
| INV-3 | **비밀은 서버에만**: 키는 서버 프로세스의 .env + Authorization 헤더에만. 브라우저로 절대 내려보내지 않음. WS 프레임/HTML/JS에 키 노출 금지 |
| INV-4 | **TUI는 디버그 sink로 유지**(삭제 아님). chat.rs 저유지보수로 보존 - 엔진 라이브 디버깅용 |
| INV-5 | **엔진은 blocking 유지**: axum=tokio(async)지만 엔진 코어는 blocking(reqwest blocking + thread::scope). 둘은 채널로 브리지(엔진 워커 스레드 ↔ async WS task). 엔진 안에 async 미도입(CLAUDE.md 기조 유지) |

## 2. Goals / Non-goals

### Goals
- (G1) **axum WebSocket 서버**: 연결 시 LiveSession 1개를 구동(또는 연결), 엔진 이벤트(발화/강도/흐름/식힘/생각중)를 JSON 프레임으로 push, 사람 입력 프레임을 받아 `submit_human`.
- (G2) **엔진↔async 브리지**: blocking LiveSession을 전용 스레드에서 구동하고, `tokio::sync::mpsc`(또는 std mpsc + bridge task)로 WS와 양방향 연결. 엔진 코어 무수정.
- (G3) **이벤트 직렬화 어댑터**: 엔진 Event/강도/FlowMetric/mu_scale을 web 프레임 스키마로(serde - 엔진이 이미 NDJSON/serde 보유, 작은 어댑터).
- (G4) **정적 프런트엔드 1장**: 채팅 로그(auto-scroll) + 사이드바 게이지(λ를 CSS 트랜지션으로 애니메이션, θ 마커, 흐름·식힘) + 입력창. 프레임워크 없이 HTML/CSS/JS로 프로덕션 외형 도달.
- (G5) **수직 슬라이스 먼저**: 엔진 push → 브라우저 렌더 → 사람 입력 → 엔진까지 한 슬라이스로 아키텍처 증명 후 폴리시.

### Non-goals (이 트랙 범위 밖, 이후)
- ❌ 멀티세션/멀티룸 동시 서빙, 인증, 호스팅(외부 배포). 시작은 **localhost 단일 세션**. 단 WS 프로토콜은 멀티세션 확장 가능하게 설계.
- (폐기됨 2026-06-03) ~~SSR/프레임워크 도입 거절, 정적 1장으로 시작~~ → **Kimi 초안(Vite+React+TS) 채택**으로 변경. 단 **SSR은 여전히 안 한다**(SPA + WS). Vite 빌드 산출물(`web/dist`)을 axum이 정적 서빙.
- ❌ 캐릭터 스프라이트(persona-ui §4) - web에서 더 잘 되지만 별도 대형 항목.
- ❌ `/invite`·persona 비주얼 명령 UI - 채팅 루프 안정화 후.
- ❌ TUI 신규 기능 - TUI는 동결(디버그용 현상 유지).

## 3. 아키텍처 델타

```
[LLM 백엔드(cloud/vLLM)]  ←(키, 서버에만)
        ▲ blocking reqwest
        │
[BackendPool] ← [LiveSession]  (blocking, 전용 스레드)
        │  events ▼      ▲ human input
   tokio mpsc 브리지 (blocking thread ↔ async)
        │  JSON 프레임 ▼  ▲
[axum WebSocket task]
        │  ws ▼          ▲
[브라우저: 정적 HTML/CSS/JS]  (키 없음)
```

| 구조 | 변경 |
|------|------|
| 엔진 코어 | **무수정**. LiveSession 재사용(이미 워커 스레드 + mpsc, 논블로킹 생성) |
| 신규 `src/web.rs`(가칭) | axum 라우터(정적 파일 서빙 + `/ws` 업그레이드), WS task ↔ LiveSession 브리지 |
| `web/` Kimi 초안(이미 존재) | Vite + React + TS + Tailwind + shadcn/ui + three.js SPA. `npm run build` → `web/dist`. axum이 `dist`를 정적 서빙(개발 중에는 Vite dev server + `/ws` 프록시). 채팅/사이드바/입력 + 엔진상태 패널 |
| 신규 프레임 스키마 | 서버→클라(utterance, intensities, flow, mu_scale, pending) / 클라→서버(human_message). serde JSON |
| `Cargo.toml` | `axum` + `tokio`(+ ws feature). **feature flag 뒤**에 둬 기본 빌드/CI/골든은 무영향(`--features web`) |
| `main.rs` | `--web [--port N] [--host H]` 플래그(opt-in). 기본 host `0.0.0.0`(LAN 허용), 기본 port 예: 8080. 기본 실행·`--chat`·`--headless`는 그대로 |
| `chat.rs`(TUI) | 강등(디버그 sink). 변경 없음 |

전송 결정은 §5.

## 4. Phases

| Phase | 내용 | 의존성 | 골든 |
|---|---|---|---|
| **P1 수직 슬라이스** | axum WS 라우트 1개 + 엔진 브리지 + Kimi 초안(`web/`) 배선(채팅 로그 + λ 게이지 + 입력 + 엔진상태 패널을 실 WS 프레임에 연결). 엔진 push→렌더→입력→엔진 한 바퀴 증명. 0.0.0.0 바인딩으로 LAN 접속 확인 | axum/tokio(feature flag) + web/ 빌드(npm) | 무관(새 sink) |
| **P2 프로덕션 외형** | 헤더(앱·방·상태) + 푸터/도움 + 화자별 색 테마 + 스크롤백 + 애니 스피너 + 빈/에러 상태 + 리사이즈 견고 | shadcn/ui 컴포넌트 | 무관 |
| **P3 생동감 디테일** | λ-band 상태 표현(멍/들썩/곧), 사람입력 시 게이지 일제 리셋 강조(persona-ui §5), 흐름/식힘 시각화 | dep 0 | 무관 |
| **P4 명령/방 운영** | `/invite`·`/help`·`/persona random` + 미리보기 모달(persona-ui §3). 멀티룸은 이후 | - | 무관 |
| **P5 캐릭터 비주얼** | persona-ui §4 스프라이트/포즈를 web 렌더로(시그니처 볼거리). 별도 대형 트랙 | - | 무관 |

P1만으로 "제대로 만든 앱" 인상의 상당 부분이 나온다(스크롤 채팅 + 애니 게이지 + 깔끔한 입력 + URL 접속).

## 5. 전송 선택: WebSocket (결정)

**채택: WebSocket** (axum 내장 WS).

근거(2026-06-03 사용자 지적 "채팅앱이면 ws로 바로가는 것도"):
- 채팅앱은 **양방향·빈번**(엔진 발화 push + 사람 입력)이라 단일 duplex 연결이 자연스럽다.
- 설계 §8 "어느 틱에든 인터럽트", 타이핑 인디케이터, presence가 duplex에 맞는다.
- axum WS 업그레이드 핸들러 하나면 됨. SSE+POST는 스트림/입력 **두 채널을 조율**해야 함.

거절된 대안 **SSE + POST**: 세우기 가장 단순(엔진→브라우저 SSE, 입력 POST), 자동 재연결·프록시 친화. 하지만 양방향 채팅엔 두 채널 조율 비용 + 인터럽트/타이핑 표현이 어색. 단순함 이득이 채팅앱 맥락에선 작다.

WS의 유일한 추가 비용: ping/pong keepalive + 재연결 로직을 직접 써야 함(SSE는 내장). localhost 단일 세션 시작 단계에선 무시 가능.

## 6. 범위/시작점 (사용자 결정 대기)

- 시작(확정 2026-06-03): **localhost + LAN 단일 세션**. axum `0.0.0.0:PORT` 바인딩으로 같은 공유기(192.168.1.X)의 다른 기기에서도 접속. 인증·멀티세션 없음(신뢰된 홈 LAN 가정). 프로토콜은 멀티세션 확장 가능하게 설계.
- 외부 접속: 사용자가 **Cloudflare 터널**로 처리한다(서버는 0.0.0.0만 바인딩, 포트포워딩/호스팅 비관여).
- 나중: 외부 공개 호스팅·멀티세션·인증 - auth/세션/배포 추가(그때 인증 필수).

## 7. 위험과 대응

| 위험 | 대응 |
|------|------|
| 골든 깨짐 | web은 새 sink. driver/headless 불침투. feature flag로 기본 빌드 무영향. 도입 후 골든 5종 재확인 |
| 키 노출 | 키는 서버 .env + Authorization만. 프레임/JS/HTML 점검(INV-3) |
| async 전염 | 엔진 blocking 유지, 채널 브리지로 격리. 엔진 코어에 async 금지(INV-5) |
| 표면 증가(서버+전송+프런트+빌드) | 정적 1장 + feature flag + 수직 슬라이스 우선으로 최소화. 프레임워크 미도입 |
| TUI 방치로 부패 | 디버그 sink로 동결·유지, 회귀는 기존 chat 테스트로 |
| 멀티세션 성급 도입 | localhost 단일부터. 프로토콜만 확장 여지 남김 |
| LAN 바인딩(0.0.0.0) 노출 | 신뢰된 홈 LAN 가정. 키는 서버에만(INV-3, 브라우저로 안 감). 외부 노출은 Cloudflare 터널로만(직접 포트포워딩 금지). 공개 호스팅 단계에선 인증 필수(그때 추가) |

## 8. 산출물

- 이 문서(플랜 v0.9). 착수 시 §4를 task로 분해(`salon-web-frontend-task-NN.md`).
- web 트랙 한 줄: Rust 엔진은 그대로, axum WebSocket으로 엔진 리듬을 브라우저에 push하고 사람 입력을 받아, 외부 사용자에게 "제대로 된 앱"으로 보이게 한다. TUI는 디버그로 남고 골든은 안 깨진다.
