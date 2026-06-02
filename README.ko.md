[English](README.md) · **한국어**

# tunaSalon

![Rust](https://img.shields.io/badge/Rust-2021-CE422B?logo=rust&logoColor=white)
![status](https://img.shields.io/badge/status-v0.1-blue)
![tests](https://img.shields.io/badge/tests-27%20passing-brightgreen)
![no LLM required](https://img.shields.io/badge/v0.1-no%20LLM%20required-8A2BE2)
![determinism](https://img.shields.io/badge/output-deterministic-informational)

로컬 LLM 페르소나들을 한 방에 풀어놓고 잡담시키는 터미널 앱. 단, 진짜 주인공은 페르소나가 아니라 **누가 언제 말하고 언제 입을 다무는지**를 정하는 대화 흐름 엔진이다.

발화를 설계하는 건 쉽다. 침묵을 설계하는 게 어렵다. 좀 거꾸로 된 프로젝트다.

---

## 그냥 페르소나 채팅이랑 뭐가 달라?

보통 "LLM한테 페르소나 줘서 채팅" 방식은 매 턴 다 같이 대답한다. 라운드로빈이거나, 호명된 애가 답하거나. 타이밍 감각도, 침묵도, "지금은 그냥 듣고 있을래" 같은 결정도 없다.

tunaSalon은 각 페르소나가 **말하고 싶은 욕구**를 가진다. 이 욕구가 시간에 따라 차오르고 식는다. 아무의 욕구도 충분히 높지 않으면 방은 조용해진다. 둘이 동시에 말하고 싶어지면 누가 낄지를 정해서 한 명만 뽑는다. 대화가 알아서 달아올랐다 식는다.

대사 규칙을 짜 넣는 게 아니다. 손잡이 몇 개를 돌리고 **지켜보면**, 성격이 리듬으로 떨어진다 — 수다쟁이, 조용한 애, 가끔만 끼어드는 애.

---

## 알고리즘, 쉽게

작은 부품 네 개면 끝이다.

1. **μ (수다력)** — 페르소나마다 정하는 기본 발화 성향. 0~1.
2. **λ (욕구)** — 매 틱 μ 쪽으로 회복하고, 방금 말한 사람은 뚝 떨어진다(독점 방지). *(self-exciting point process, Hawkes의 단순화판.)*
3. **θ (침묵 게이트)** — 매 틱 아무의 λ도 θ를 못 넘으면 방은 조용하다. θ를 올릴수록 침묵이 잦아진다.
4. **누구를? (RRF)** — θ를 넘은 후보가 여럿이면 신호 셋을 섞어 한 명을 뽑는다: 누가 제일 말하고 싶나(λ), 최근에 누가 적게 말했나(공정성), 그리고 약간의 무작위. *(Reciprocal Rank Fusion.)*

매 틱 루프는 이게 전부다:

```
모든 λ를 μ 쪽으로 살짝 회복  →  게이트: 아무도 θ 못 넘으면 침묵
                              →  넘었으면 RRF로 한 명 선택
                              →  그 사람 λ를 떨어뜨림  →  반복
```

리듬을 만드는 데 LLM은 필요 없다. v0.1은 내용 없는 가짜 한마디만 쓴다. 성격은 **타이밍**에 들어있다.

### 손잡이가 뭘 하는지 (실제 출력)

`cargo run -- --sweep` 가 같은 시드로 θ와 k를 훑은 결과다. μ는 friend 0.80, chaos 0.70, summarizer 0.25.

```
θ=0.40  침묵   0   friend 100  chaos 100  summarizer 0   # 게이트가 헐거움 → 다 통과, 쉴 새 없는 핑퐁
θ=0.65  침묵 100   friend  62  chaos  38  summarizer 0   # 게이트가 물림 → 발화·침묵 리듬, μ 차이가 드러남
θ=0.78  침묵 171   friend  29  chaos   0  summarizer 0   # 게이트가 빡빡 → 제일 수다스러운 애만 겨우 한마디
```

같은 μ인데 θ 하나로 "쉴 새 없는 방 / 리듬 있는 방 / 거의 조용한 방"이 갈린다. (참고: 발화 분산은 사실 k보다 *공정성 신호*가 좌우한다 — 그래서 위 표에서 k를 바꿔도 분포가 거의 안 변한다. 이런 걸 미터로 보면서 알아간다.)

### 미터

`cargo run` 으로 TUI를 띄우면 λ 막대가 θ 선(`|`)에 다가갔다 멀어지고, 누가 왜 말했는지가 실시간으로 보인다.

```
┌events──────────────────────────────────────┐┌gauges────────────────────┐
│t8 (silence)                                ││Chaos Guest               │
│t9 (silence)                                ││########|.... 0.63        │
│t10 Friendly Regular                        ││Friendly Regular          │
│                                            ││########|.... 0.67        │
│                                            ││Quiet Summarizer          │
│                                            ││###.....|.... 0.25        │
│                                            ││speak 11  silence 6       │
└────────────────────────────────────────────┘└──────────────────────────┘
┌──────────────────────────────────────────────────────────────────────────┐
│tick 10 | len 17 | Friendly Regular | reason: intensity   [q] quit [space]  │
└──────────────────────────────────────────────────────────────────────────┘
```

---

## 해보기

[Rust](https://rustup.rs) 만 있으면 된다. (v0.1은 LLM도 네트워크도 필요 없다.)

```bash
cargo run                                   # 미터를 라이브로 보기 (TUI). q로 종료, space로 일시정지
cargo run -- --headless --ticks 200         # 결정적 NDJSON, 틱당 한 줄
cargo run -- --sweep                        # θ × k 격자로 리듬 비교
cargo run -- --theta 0.7 --k 5 --beta 0.4   # 손잡이 돌려보기
cargo test                                  # 27개 테스트 (스모크 게이트 포함)
```

손잡이: **μ**(페르소나별 수다력) · **θ**(침묵 임계) · **k**(RRF 동점 처리) · **β**(욕구 회복 속도).
같은 `--seed` 면 출력은 매번 똑같다(결정적). 그래서 헤드리스로 자동 검증이 된다.

---

## 지금 상태

**v0.1 (지금):** 리듬 엔진. 아직 LLM 없음 — 가짜 한마디로 *타이밍*이 살아있는지부터 검증한다(수다/조용/가끔 끼어드는 페르소나, 창발하는 침묵). 결정적 + 디버그 미터. Rust, 27개 테스트, 스모크 게이트 green.

**다음:**
- **v0.2 — 케미(α):** 누가 누구를 자극하나. ENTP critic이 INTJ를 들쑤시고, poet은 감정 발화에 반응한다. 방 프리셋(calm/pub/argument/chaos)도.
- **v0.3 — 진짜 로컬 LLM:** Ollama 페르소나가 실제 대사를 생성. 화자 선택은 엔진이, 내용만 모델이. 작은 모델이 페르소나를 유지하는지(persona collapse) 확인.

---

## 왜

멀티에이전트 LLM 데모는 거의 다 과제를 풀고 끝난다. 답이 나오면 종료. 그런데 진짜 잡담엔 과제도 종료 조건도 없다 — 그냥 흐르다 식는다. 그건 다른 종류의 엔진이 필요했고, 그래서 찔러볼 수 있는 걸 하나 만들었다.

---

*알고리즘 출처: Multivariate Hawkes process (self-exciting point process), Reciprocal Rank Fusion, AutoGen GroupChat 패턴. 자세한 설계는 [docs/reference/salon-engine-design.md](docs/reference/salon-engine-design.md).*
