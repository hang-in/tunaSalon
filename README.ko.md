[English](README.md) · **한국어**

# tunaSalon

![Rust](https://img.shields.io/badge/Rust-2021-CE422B?logo=rust&logoColor=white)
![status](https://img.shields.io/badge/status-v0.10%20%2B%20web-blue)
![tests](https://img.shields.io/badge/tests-306%20passing-brightgreen)
![LLM optional](https://img.shields.io/badge/LLM-%EC%84%A0%ED%83%9D%EC%82%AC%ED%95%AD%2C%20%EA%B8%B0%EB%B3%B8%20%EA%BA%BC%EC%A7%90-8A2BE2)
![determinism](https://img.shields.io/badge/output-deterministic-informational)

**tunaSalon**은 여러 LLM 페르소나가 한 주제를 두고 말하고, 끼어들고, 반박하고, 때로는 조용히 듣는 브라우저 토론 앱입니다.

보통의 페르소나 채팅은 참가자들이 순서대로 답하거나, 매 턴 전원이 한마디씩 합니다. tunaSalon은 그렇게 움직이지 않습니다. 각 페르소나는 자기만의 **발언 욕구**를 갖고, 그 욕구가 충분히 올라왔을 때만 말합니다. 아무도 말할 만큼 달아오르지 않으면 방은 조용해집니다.

> 말을 시키는 건 쉽습니다. 어려운 건 자연스럽게 입을 다물게 만드는 쪽입니다.

tunaSalon의 핵심은 “무슨 말을 생성할까”보다 **누가 지금 말해야 하는가**입니다.  
LLM은 문장을 채우고, 엔진은 타이밍을 정합니다.

---

## 무엇이 다른가

일반적인 그룹 채팅형 LLM 앱은 대개 다음 중 하나로 동작합니다.

- 정해진 순서대로 한 명씩 답합니다.
- 사용자가 부른 페르소나만 답합니다.
- 매 턴 모든 페르소나가 한마디씩 합니다.

이 방식은 구현은 쉽지만, 토론처럼 보이기 어렵습니다. 실제 대화에는 침묵, 간섭, 머뭇거림, 독점 방지, 갑작스러운 재점화가 있습니다.

tunaSalon은 대화 내용을 만들기 전에 먼저 **대화의 리듬**을 만듭니다.

| 일반 페르소나 채팅 | tunaSalon |
|---|---|
| 순서 또는 호출 기반 발화 | 발언 욕구 기반 발화 |
| 매 턴 누군가 반드시 말함 | 말할 사람이 없으면 침묵 |
| 참가자 전원이 자주 답함 | 한 번에 한 명만 선택 |
| 페르소나는 주로 프롬프트 차이 | 페르소나는 말투 + 발언 리듬 차이 |
| 대화 흐름이 프롬프트에 의존 | 흐름 엔진이 타이밍을 제어 |

결과적으로 같은 모델을 써도 페르소나는 다르게 움직입니다.  
수다스러운 사람은 자주 끼어들고, 과묵한 사람은 오래 듣다가 특정 분위기에서만 입을 엽니다.

---

## 지금 할 수 있는 것

tunaSalon은 두 트랙으로 동작합니다.

1. **엔진 트랙**  
   LLM 없이도 실행되는 결정적 대화 흐름 시뮬레이터입니다. `cargo run`으로 TUI 미터를 보고, `--headless`로 바이트 동일한 NDJSON 출력을 검증할 수 있습니다.

2. **웹 앱 트랙**  
   `web` / `redis-bus` feature 뒤에 있는 브라우저 토론 앱입니다. axum WebSocket + React 기반으로, 주제 입력부터 페르소나 토론, 사용자 참여, 종료 리포트, 공유 링크까지 제공합니다.

### 주요 기능

| 기능 | 설명 |
|---|---|
| 주제 기반 토론 | 주제를 입력하면 방이 열리고 페르소나들이 토론을 시작합니다. |
| 결정적 토론 모드 | 주제를 `Courtroom`, `PolicyDuel`, `MoralDilemma`, `Forecasting`, `DesignReview`, `CasualBanter` 등으로 매핑합니다. |
| 성격 있는 페르소나 | 혈액형, MBTI, 별자리 등 4축으로 성향과 말투를 조립합니다. |
| 랜덤/수동 방 생성 | room id 기반 결정적 랜덤 3명 또는 직접 구성한 2~3명으로 방을 만들 수 있습니다. |
| 사용자 1급 참여 | 사용자가 끼어들면 페르소나들이 해당 발언을 중심으로 반응합니다. 닉네임을 부르면 해당 페르소나가 우선 반응합니다. |
| 발언 게이지 | 참가자별 발언 욕구 `λ`가 침묵 문턱 `θ`에 대해 얼마나 차올랐는지 볼 수 있습니다. |
| 종료 리포트 | 토론 종료 후 결론, 참가자별 입장, 합의점, 갈린 지점을 마크다운으로 정리합니다. |
| 읽기전용 공유 | 공유를 누른 방만 토큰 링크로 외부 공개됩니다. 로그인 없이 전사를 읽을 수 있습니다. |
| 이전 토론 아카이브 | 지난 방의 주제, 참가자, 히스토리, 결론을 복원할 수 있습니다. |
| Redis 멀티세션 | 여러 브라우저/인스턴스가 같은 방을 안정적으로 공유하도록 command/event 스트림, owner lease, presence를 제공합니다. |

---

## 엔진 요약

tunaSalon의 대화 흐름은 네 값으로 제어됩니다.

| 값 | 의미 |
|---|---|
| `μ` | 기본 말수. 페르소나가 원래 얼마나 수다스러운지 나타냅니다. |
| `λ` | 현재 발언 욕구. 시간이 지나면 `μ`를 향해 회복하고, 말하고 나면 떨어집니다. |
| `θ` | 침묵 문턱. 어떤 `λ`도 이 값을 넘지 못하면 방은 조용해집니다. |
| `RRF` | 화자 선택. 여러 명이 말할 자격을 얻었을 때 실제 화자 한 명을 고릅니다. |

매 틱의 흐름은 단순합니다.

```text
λ를 μ 쪽으로 회복
→ θ를 넘은 후보 확인
→ 후보가 없으면 침묵
→ 후보가 여러 명이면 RRF로 한 명 선택
→ 선택된 화자의 λ를 낮춤
→ 반복
```

이 구조 덕분에 tunaSalon은 “모두가 매번 말하는 패널”이 아니라, 말하고 싶은 사람이 차례로 올라오고, 방이 식으면 조용해지는 토론장을 만들 수 있습니다.

---

## 손잡이를 돌리면

`cargo run -- --sweep`은 같은 seed에서 침묵 문턱 `θ`를 바꿔봅니다.  
아래 예시는 `friend μ=0.80`, `chaos μ=0.70`, `summarizer μ=0.25`를 고정한 결과입니다.

```text
θ=0.40  silence   0   friend 100  chaos 100  summarizer 0
θ=0.65  silence 100   friend  62  chaos  38  summarizer 0
θ=0.78  silence 171   friend  29  chaos   0  summarizer 0
```

`μ`는 그대로인데 `θ` 하나만 바꿔도 방은 다음처럼 달라집니다.

| `θ` | 결과 |
|---|---|
| 낮음 | 거의 쉬지 않고 말합니다. |
| 중간 | 말과 침묵의 리듬이 생깁니다. |
| 높음 | 가장 수다스러운 페르소나만 겨우 말합니다. |

---

## 케미: 교차 자극 `α`

v0.2부터는 페르소나 사이의 **교차 자극 `α`**가 들어갑니다.  
한 명이 말하면 다른 페르소나의 발언 욕구가 올라갑니다. 누가 누구를 얼마나 자극하는지가 방의 케미가 됩니다.

```text
preset=Calm      silence 99   friend 67  chaos 34  summarizer  0
preset=Argument  silence  0   friend 76  chaos 76  summarizer 48
```

같은 `summarizer(μ=0.25)`라도 차분한 방에서는 한 번도 말하지 않고, 논쟁적인 방에서는 48번 끼어듭니다.  
페르소나는 그대로지만, 방 분위기가 과묵한 사람을 끌어낸 것입니다.

---

## TUI 미터

`cargo run`으로 TUI를 열면 각 페르소나의 발언 욕구가 실시간으로 보입니다.  
막대는 `λ`, 세로선은 침묵 문턱 `θ`입니다.

```text
┌events──────────────────────────────────────┐┌gauges────────────────────┐
│t8 (silence)                                ││Chaos Guest               │
│t9 (silence)                                ││########|.... 0.63        │
│t10 Friendly Regular                        ││#########.... 0.67        │
│                                            ││Quiet Summarizer          │
│                                            ││###.....|.... 0.25        │
│                                            ││speak 11  silence 6       │
└────────────────────────────────────────────┘└──────────────────────────┘
```

방이 왜 조용해졌는지, 왜 특정 페르소나가 독점하는지, 어느 값이 지나치게 높은지를 숫자로 확인하면서 조정할 수 있습니다.

---

## 브라우저 토론 앱

웹 앱은 엔진 위에 올라간 제품 트랙입니다.

```bash
cargo run --features "web redis-bus" -- --web --topic "AI 판사가 공정할까?"
```

브라우저에서는 주제를 넣고, 방을 만들고, 페르소나 토론을 읽고, 직접 끼어들 수 있습니다.

### DebatePlan

`src/debate/`의 연출자 레이어는 주제를 결정적으로 토론 모드에 매핑합니다.  
LLM 메타콜로 방식을 고르는 것이 아니라, 재현 가능한 규칙으로 모드를 선택합니다.

| 모드 | 예시 주제 |
|---|---|
| `Courtroom` | AI 판사가 공정할까? |
| `PolicyDuel` | AI 규제와 오픈소스 |
| `MoralDilemma` | 편의를 위해 감시를 허용해도 되는가? |
| `PersonalStakes` | 가족의 선택을 어디까지 존중해야 하는가? |
| `Forecasting` | 5년 뒤 개발자는 어떻게 바뀔까? |
| `DesignReview` | 이 아키텍처는 유지보수 가능한가? |
| `CasualBanter` | 민초는 음식인가, 치약인가? |

### 형식 변주

매 턴이 같은 길이의 에세이가 되지 않도록 발언 형식을 바꿉니다.

- 교차신문
- 스틸맨 후 반박
- 구체 사례 하나
- 측정 가능한 임계값 제시
- 조건부 양보
- 짧은 한두 문장 반응

토론이 같은 자리에서 맴돌면 twist 카드가 새 국면을 넣습니다.  
예를 들어 오판 통계, 규제 부담, 떠난 유지보수자, 비용 증가 같은 카드를 넣어 다시 반응을 유도합니다.

### 친구끼리 따지는 톤

토론의 분석 초점은 유지하되, 말투는 지나치게 딱딱하지 않게 낮춥니다.  
법정처럼 굳은 문장이 아니라 친구끼리 “근데 그건 좀 다르지 않나?” 하고 따지는 쪽에 가깝습니다.

---

## Redis 멀티세션

`redis-bus` feature를 켜면 Redis는 멀티세션 조정 레이어로 사용됩니다.

```bash
SALON_REDIS_URL=redis://127.0.0.1:6379 \
cargo run --features "web redis-bus" -- --web
```

Redis가 담당하는 것은 다음입니다.

| 역할 | 설명 |
|---|---|
| command stream | 방에 들어오는 명령 전달 |
| event stream | 방에서 발생한 이벤트 전파 |
| owner lease | 방당 단일 writer 유지 |
| presence | 접속자 상태 관리 |

Redis는 기억 저장소가 아닙니다.  
영속 데이터의 SSOT는 SQLite입니다.

| 저장소 | 역할 |
|---|---|
| `memory.db` | 기억, 회상, friend engine 데이터 |
| `rooms.db` | 방, 참가자, 히스토리, 공유 정보 |
| Redis | 휘발성 멀티세션 코디네이션 |

Redis를 비워도 히스토리는 사라지지 않습니다.

---

## 실행하기

[Rust](https://rustup.rs)만 있으면 기본 실행이 가능합니다.  
기본 경로는 LLM도 네트워크도 필요 없습니다.

```bash
cargo run
```

### 주요 명령

```bash
cargo run
cargo run -- --chat
cargo run --features "web redis-bus" -- --web --topic "AI 판사가 공정할까?"
cargo run -- --headless --ticks 200
cargo run -- --sweep
cargo run -- --room argument
cargo run -- --room chaos --fsm
cargo run -- --theta 0.7 --k 5 --beta 0.4
cargo run -- --llm
cargo run --features friend-engine -- --chat
cargo run --features friend-engine-semantic -- --chat
cargo run --example persona_collapse
cargo run --example mixed_bench
cargo run --example chat_demo
cargo test
```

### 명령 설명

| 명령 | 설명 |
|---|---|
| `cargo run` | TUI 미터 실행. `q` 종료, `space` 일시정지. |
| `cargo run -- --chat` | 터미널에서 대화형 방 실행. 실제 터미널 필요. |
| `cargo run --features "web redis-bus" -- --web` | 브라우저 토론 앱 실행. |
| `cargo run -- --headless --ticks 200` | 틱당 한 줄 결정적 NDJSON 출력. |
| `cargo run -- --sweep` | `θ × k` 격자와 방 프리셋 비교. |
| `cargo run -- --room argument` | 논쟁적인 방 프리셋 실행. |
| `cargo run -- --room chaos --fsm` | 케미 + 같은 화자 2연속 금지 FSM 실행. |
| `cargo run -- --llm` | LLM opt-in 실행. 기본은 꺼져 있습니다. |
| `cargo run --features friend-engine -- --chat` | 형태소 + SQLite BM25 기반 기억 회상 사용. |
| `cargo run --features friend-engine-semantic -- --chat` | BGE-M3 기반 의미 회상까지 사용. |
| `cargo test` | 테스트 실행. |

### 옵션

| 옵션 | 의미 |
|---|---|
| `--theta` | 침묵 문턱 |
| `--k` | RRF 1등 쏠림 완화 |
| `--beta` | 발언 압력 회복/감쇠 속도 |
| `--room` | 방 분위기 프리셋 |
| `--seed` | 결정적 출력용 seed |

같은 `--seed`면 출력은 매번 바이트 단위로 동일합니다.  
headless 출력은 이 성질을 이용해 자동 검증합니다.

---

## Feature flags

제품 트랙과 실험 트랙은 feature 뒤에 있습니다.  
기본 빌드와 headless 골든 출력은 손상하지 않는 것을 원칙으로 합니다.

| Feature | 설명 |
|---|---|
| `web` | axum WebSocket + React 브라우저 토론 앱 |
| `redis-bus` | Redis 기반 멀티세션 코디네이션 |
| `friend-engine` | 형태소 분석 + SQLite BM25 기반 기억 회상 |
| `friend-engine-semantic` | BGE-M3 ONNX + HNSW + BM25/벡터 RRF 의미 회상 |

---

## 현재 상태

현재 tunaSalon은 v0.1~v0.10 엔진 위에서 브라우저 토론 앱으로 동작합니다.

| 영역 | 상태 |
|---|---|
| 엔진 | Hawkes 기반 발언 욕구, 침묵 게이트, RRF 화자 선택, 케미 프리셋 |
| TUI | 발언 게이지, 이벤트 로그, headless deterministic 출력 |
| LLM | opt-in. 기본 꺼짐. 로컬/클라우드 백엔드 혼합 가능 |
| friend engine | 형태소 + BM25 회상, 의미 회상 옵션 |
| 웹 앱 | WebSocket 토론, 페르소나 구성, 사용자 참여, 종료 리포트, 공유 링크, 아카이브 |
| 멀티세션 | Redis command/event stream, owner lease, presence |
| 테스트 | 기본 306 passing, friend-engine 319, semantic 347, web 356 |

자세한 버전별 흐름과 실제 출력은 [HISTORY.ko.md](HISTORY.ko.md)에 있습니다.

---

## 로드맵

다음 작업은 별도 트랙으로 진행됩니다. 순서는 고정되어 있지 않습니다.

### 연출자 레이어

- 숨은 페르소나 목표
- 공개 입장과 비공개 목적 분리
- 토픽별 실데이터 evidence 카드
- 미응답 질문 ledger
- 반복 논점 감지와 개입 강화

### friend engine

- 망각
- 캐릭터별 주관적 저장
- 방을 가로지르는 인물 인상
- 장기 기억과 방 단위 기억의 분리

### 웹 앱

- 픽셀아트 아바타
- 멀티룸
- 본인 프로필/프리셋
- 웹서치 tool use
- 공유 페이지 개선

---

## 왜 만들었나

LLM 여러 개를 한 주제에 붙이면 대부분 모두가 매번 답합니다.  
그건 토론이라기보다 답변 묶음에 가깝습니다.

실제 대화에는 타이밍이 있습니다.

- 누군가는 끼어듭니다.
- 누군가는 듣고만 있습니다.
- 같은 사람이 너무 오래 말하면 흐름이 죽습니다.
- 방이 조용해졌다가도, 누군가의 한마디로 다시 살아납니다.

tunaSalon은 이 타이밍을 먼저 설계합니다.  
페르소나는 그 위에 올라가는 목소리입니다.

말을 잘 생성하는 모델은 많습니다.  
tunaSalon이 다루는 문제는 조금 다릅니다.

> 지금 이 방에서, 누가 말해야 하는가.  
> 그리고 언제 아무도 말하지 않아도 되는가.

---

## 참고 문서

- [HISTORY.ko.md](HISTORY.ko.md) - v0.1~v0.10 엔진과 제품 트랙 walkthrough
- [docs/reference/salon-engine-design.md](docs/reference/salon-engine-design.md) - 엔진 설계 상세
- Multivariate Hawkes process - self-exciting point process 기반 발언 욕구 모델
- Reciprocal Rank Fusion - 여러 신호를 합친 화자 선택
- AutoGen GroupChat 패턴 - LLM 그룹 대화 패턴 참고

---

## 라이선스

라이선스 정보는 저장소의 `LICENSE` 파일을 확인하세요.
