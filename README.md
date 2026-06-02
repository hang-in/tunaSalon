**English** · [한국어](README.ko.md)

# tunaSalon

![Rust](https://img.shields.io/badge/Rust-2021-CE422B?logo=rust&logoColor=white)
![status](https://img.shields.io/badge/status-v0.6-blue)
![tests](https://img.shields.io/badge/tests-168%20passing-brightgreen)
![LLM optional](https://img.shields.io/badge/LLM-optional%2C%20default--off-8A2BE2)
![determinism](https://img.shields.io/badge/output-deterministic-informational)

A terminal chat room where you drop in and small-talk with LLM personas. The catch: the star isn't the personas — it's the **conversation-flow engine** that decides who speaks when, and when the room just goes quiet. v0.5 makes you a first-class participant: type something and the personas turn to react; go quiet and they drift back to their own chatter. v0.6 adds a conversation thermometer — watching whether the room is converging or still alive.

Designing speech is easy. Designing silence is hard. This project is a little backwards on purpose.

---

## How is this different from just persona chat?

The usual "give an LLM a persona and chat" setup has everyone answer every turn — round-robin, or whoever got prompted. No sense of timing, no silence, no "I'll just listen this round."

In tunaSalon each persona has an **urge to speak** that rises and falls over time. If nobody's urge is high enough, the room goes quiet. If two want in at once, a tie-break picks one. Conversations heat up and cool down on their own.

You don't script the dialogue rules. You turn a few knobs and **watch** — and personalities fall out as rhythms: the chatterbox, the quiet one, the one who only chimes in sometimes.

---

## How it works (the simple version)

Four small parts:

1. **μ (chattiness)** — each persona's baseline urge to talk, 0–1.
2. **λ (urge)** — recovers toward μ each tick, and drops right after they speak, so nobody hogs the floor. *(A stripped-down Hawkes self-exciting process.)*
3. **θ (silence gate)** — each tick, if no λ clears θ, the room stays quiet. Higher θ means more silence.
4. **who? (RRF)** — when several clear θ, fuse three signals to pick one: who wants it most (λ), who's spoken least lately (fairness), and a pinch of randomness. *(Reciprocal Rank Fusion.)*

The whole loop, every tick:

```
nudge every λ back toward μ  →  gate: if nobody clears θ, stay silent
                             →  else pick one speaker via RRF
                             →  drop that speaker's λ  →  repeat
```

No LLM needed to get a rhythm — a fake one-word utterance is enough. The personality lives in the **timing**.

### What the knobs do (real output)

`cargo run -- --sweep` runs θ × k at a fixed seed. μ = friend 0.80, chaos 0.70, summarizer 0.25:

```
θ=0.40  silence   0   friend 100  chaos 100  summarizer 0   # gate loose → everyone passes, nonstop ping-pong
θ=0.65  silence 100   friend  62  chaos  38  summarizer 0   # gate bites → speech/silence rhythm, μ shows through
θ=0.78  silence 171   friend  29  chaos   0  summarizer 0   # gate harsh → only the chattiest gets a word in
```

Same μ, but a single θ turns the room from nonstop, to rhythmic, to nearly silent. (Aside: spread is actually governed more by the *fairness signal* than by k — which is why changing k barely moves these numbers.)

### Chemistry (v0.2)

v0.2 adds **cross-excitation (α)**: one persona speaking lifts the others' urge. Who riles up whom *is* the room's chemistry, and you pick the mood with a preset.

```
preset=Calm      silence 99   friend 67  chaos 34  summarizer  0   # weak α → the quiet one stays quiet
preset=Argument  silence  0   friend 76  chaos 76  summarizer 48   # strong α → cross-excitation drags the quiet one in
```

The same summarizer (μ = 0.25) never speaks in Calm but speaks 48 times in Argument. The personas are identical — the **room's mood pulls the quiet one into the conversation**.

### The meter

`cargo run` opens the TUI. The λ bars drift toward and away from the θ line (`|`), and you see who spoke and why, live:

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

## Local LLMs (v0.3)

v0.3 wires in Ollama so personas generate actual lines. The engine still decides **who speaks and when** — deterministic as always. The LLM only fills in the content.

Default run stays LLM-off (FakeBackend) — byte-identical to v0.1 output, zero network needed. Pass `--llm` to opt in.

When real text is in play, two content-based RRF signals activate: **interest** (how much a new topic pulls a persona in) and **echo** (whether a persona is still processing what was just said). These sit on top of the existing λ/fairness/randomness signals.

A dedicated example, `persona_collapse`, puts the same model under two different persona prompts and logs both outputs side by side — watching whether a small model actually holds distinct personas over time or collapses toward a uniform voice.

## Concurrent / mixed-model (v0.4)

v0.4 adds a **backend pool** with two protocols:

- **Ollama** `/api/generate` — e.g. `gemma4:31b-cloud` (cloud, concurrency cap 3)
- **OpenAI-compatible** `/v1/chat/completions` — e.g. a friend's vLLM server (`qwen3.6-35b-fast`, concurrency cap 1)

**Per-persona routing** means a single room can mix models: some personas talk through one backend, some through another. A `mixed_bench` example puts both in the same room.

The live tick loop stays **sequential** (one speaker per tick, causal turn-taking). Concurrency is used for compare/bench via `generate_batch` — running the same prompt against multiple backends in parallel to compare persona tone or benchmark latency.

Defaults to cloud models — no local RAM/GPU load, the local daemon just proxies the request remotely. Local model loading is guarded against.

Per-backend semaphores enforce the concurrency cap. If a backend returns 4xx or times out, a fallback chain kicks in (next backend, or FakeBackend) — no panics.

**Real mixed-model output** (`cargo run --example mixed_bench`):

```
cloud  : gemma4:31b-cloud (cap=3)   friend : qwen3.6-35b-fast (cap=1)
routing: summarizer → friend, others → cloud
opening> 오늘 비 와서 다들 약속 취소했대. 좀 심심하네.

[friend via cloud]      비 오는 날엔 원래 좀 늘어지기 쉽지. 여기 커피나 마시면서 멍 때려.
[chaos via cloud]       그럼 우리 집 거실에서 비 구경 대회나 열까?
[summarizer via friend] 혼자 남아 있는 공간은 생각할 시간이 충분해진다.
```

The summarizer, routed to the larger friend model, reads quieter and more reflective — persona tone distinction holds even across different models.

---

## Join the room (v0.5)

v0.5 turns tunaSalon into the thing it was always building toward: a chat room you can actually join.

`cargo run -- --chat` opens a three-panel TUI: a scrolling **chat pane** on the left (all utterances, labelled by persona), a **gauge sidebar** on the right (each persona's live λ vs. the θ line), and a **text input box** at the bottom. You type; the personas react.

The mechanism is **HumanChannel**: when you send a message, it fires as a large external event in the Hawkes engine — strongly exciting every persona's urge and pulling the topic toward you (design §5). Personas that were mid-chatter turn toward you; when you go quiet, the room drifts back to its own rhythm.

LLM generation (~1.6s/line) runs off the main thread, so the UI stays responsive while a persona is "thinking." Replies come one at a time — causal turn-taking, same as always. Personas can be routed to different backends (e.g. a cloud model + a friend's vLLM `qwen3.6-35b-fast`), with fallback if one is down.

`--chat` requires a real terminal. In a non-interactive shell it prints a graceful error and exits.

**Live demo** (`cargo run --example chat_demo` — no terminal needed, runs non-interactively):

```
친구: Hey! What's up?
혼돈: I've decided to replace all your socks with slightly damp sponges. Ta-ta!
나: 안녕, 다들 비 와서 뭐해?          ← the human joins
친구: 난 그냥 집에서 뒹굴거리는 중! ㅋㅋㅋ   ← persona turns to react (switches to Korean)
```

When the human speaks, the room's attention shifts.

---

## FlowMeter — conversation thermometer (v0.6)

Is the room still alive, or is everyone just circling the same ground?

v0.6 adds a **convergence gauge**: a number between 0 and 1 that tracks whether recent utterances are pulling toward the same vocabulary (converging — repetitive, cooling) or scattering into fresh territory (diverging — lively, still warm).

Implementation is deliberately cheap: **token-overlap approximation** — average pairwise Jaccard similarity over the most recent utterances. No models required. BGE-M3 embeddings can drop in behind the same interface later when a more precise signal is needed.

This is **observe-only**: the gauge measures and displays; it does not feed back into the engine. No parameter changes, no automatic room adjustments. That feedback loop is v0.7 (MetaController) — deliberately last, because it's the least stable.

Where it shows up:
- TUI sidebar: a live convergence gauge next to the λ bars ("흐름 수렴 …").
- `cargo run --example chat_demo`: a per-line `[흐름] 수렴` readout after each utterance.

**Real `chat_demo` transcript** — convergence stays near zero because the chaos persona keeps throwing new topics:

```
혼돈: 사실 너의 발가락 사이에서 비밀 사회가 운영 중이라는 소문을 들었어...
  [흐름] 수렴 0.00
나: 얘들아 각자 자기소개 좀 해줄래?
  [흐름] 수렴 0.01
혼돈: 난 그냥 여기저기 불 지르고 다니는 혼돈의 전도사야!
  [흐름] 수렴 0.04
```

*(convergence ∈ [0,1]: 0 = all-novel / diverging / lively — 1 = repetitive / converging / cooling.)*

Determinism preserved: the metric is computed only from utterance content. In the default LLM-off run, utterances are fake and content-free, so the gauge is absent — headless golden output stays byte-identical.

---

## Try it

All you need is [Rust](https://rustup.rs). The default run needs no LLM and no network.

```bash
cargo run                                         # watch the meter live (TUI). q to quit, space to pause
cargo run -- --chat                               # join the room: interactive chat (needs a real terminal)
cargo run -- --headless --ticks 200               # deterministic NDJSON, one line per tick
cargo run -- --sweep                              # θ × k grid + preset comparison
cargo run -- --room argument                      # room mood preset (calm/pub/argument/chaos)
cargo run -- --room chaos --fsm                   # chemistry + no speaker twice in a row
cargo run -- --theta 0.7 --k 5 --beta 0.4         # turn the knobs
cargo run -- --llm                                # opt in to LLM (default cloud model, needs network)
cargo run --example persona_collapse              # same model, two personas — does it hold? (needs Ollama)
cargo run --example mixed_bench                   # cloud + friend vLLM in the same room (needs both backends)
cargo run --example chat_demo                     # non-interactive chat loop with flow readout per line
cargo test                                        # 168 tests, including the smoke gates
```

Knobs: **μ** (per-persona chattiness) · **θ** (silence threshold) · **k** (RRF tie-break sharpness) · **β** (urge recovery speed). Same `--seed` gives identical output every run, so it's verifiable headless.

---

## Status

**v0.6 (now):** FlowMeter — convergence/divergence measurement (observe-only). Token-overlap approximation (average pairwise Jaccard); live gauge in TUI sidebar and per-line readout in `chat_demo`. Default run stays deterministic and LLM-free. Rust, 168 tests, smoke gates green.

**So far:**
- **v0.1 — rhythm:** speech/silence rhythm from μ, θ, and the tie-break alone.
- **v0.2 — chemistry (α):** who riles up whom; room presets (calm / pub / argument / chaos) and persona pairings.
- **v0.3 — local LLMs:** Ollama personas generate actual lines. Engine decides who speaks; LLM fills in content. `persona_collapse` example: same model, different persona prompts.
- **v0.4 — concurrent / mixed-model:** backend pool (Ollama + OpenAI-compatible), per-persona routing, concurrency caps, fallback. `mixed_bench` example.
- **v0.5 — join the room:** human-in-the-loop chat (HumanChannel + `--chat` TUI). `chat_demo` example.
- **v0.6 — FlowMeter:** convergence/divergence measurement (observe-only). Token-overlap approximation; BGE-M3 embeddings later. Live gauge in TUI + `chat_demo` readout.

**Next:**
- **v0.7 — MetaController:** macro→micro feedback — cool the room when it converges; ramp it back up when it stalls. Deliberately last, weak gain.

*(Deferred: long-term memory / friend engine — design notes in `docs/temp/`.)*

---

## Why

Most multi-agent LLM demos solve a task and stop. Real small talk has no task and no end — it just flows and fades. That needs a different kind of engine, so here's one you can poke at.

---

*Sources: Multivariate Hawkes process (self-exciting point process), Reciprocal Rank Fusion, AutoGen GroupChat pattern. Full design notes in [docs/reference/salon-engine-design.md](docs/reference/salon-engine-design.md).*
