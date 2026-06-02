**English** · [한국어](README.ko.md)

# tunaSalon

![Rust](https://img.shields.io/badge/Rust-2021-CE422B?logo=rust&logoColor=white)
![status](https://img.shields.io/badge/status-v0.9-blue)
![tests](https://img.shields.io/badge/tests-222%20passing-brightgreen)
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

## MetaController — closing the feedback loop (v0.7)

v0.6 measured convergence. v0.7 **acts on it**.

When the conversation **converges** — people circling the same ground — the engine lowers **μ (base chattiness)** via a cooling factor `mu_scale` (∈ [floor, 1]). The room speaks less, lets silence stretch, and winds down on its own. When talk is diverging and lively, the controller leaves μ alone.

This is a **heuristic** macro→micro layer, not an LLM. One direction only: convergence → cooling.

**Stability is the main design concern** — a feedback loop can oscillate or get stuck, and this is the least stable part of the whole system. Two guards are in place:
- **Weak gain by default**: the cooling response is deliberately slow (tunable via `SALON_META_GAIN`).
- **Floor**: `mu_scale` never drops below ~0.4, so the room never goes permanently silent.

**Observe it**: the TUI sidebar and `chat_demo` show a "식힘 ×{value}" (cooling) line. `1.00` means no cooling is active.

**Content-gated / golden preserved**: with no LLM content (the default deterministic run), there is no convergence signal, so `mu_scale` stays at 1.0 and the engine is byte-identical to before. The deterministic headless path is unchanged.

A note worth stating: in the chaos-room demo, the conversation keeps diverging, so the cooling gauge idles at `식힘 ×1.00`. That is correct behavior — cooling only engages when a conversation actually circles a topic long enough to cross the convergence threshold. A room that never converges never gets cooled.

---

## Long-term memory — friend engine, first increment (v0.8)

The original engine-layer roadmap (v0.1–v0.7) is complete. v0.8 opens a separate track: **personas that remember**. A persona who's been in the room with you starts recalling what was said and weaves it back into what it says next.

This first increment is deliberately small:
- **Participation-based memory**: an in-memory store of events `{room, ts, speaker, content}` plus who was present in each room. A persona can only recall from rooms it actually sat in — no recalling a conversation it wasn't part of.
- **Keyword recall**: token-overlap scoring (reusing the FlowMeter tokenizer) picks the top-K past lines for the current context. Injected into the recall slot the v0.3 context interface already reserved.
- **Recall eval harness**: the real payoff. A headless scenario plants a known fact (SSOT) plus distractors across rooms, then auto-scores the retrieval layer for recall/precision and participation isolation — deterministically.

What's **not** here yet (later): BGE-M3 semantic search, SQLite persistence (L1), forgetting, subjective per-persona storage, cross-room impressions of people.

**Content-gated / golden preserved**: recall is wired into the **live chat path only**. The deterministic headless/driver path never injects recall, so with no LLM content there are no events, no recall, and the output stays byte-identical to v0.1.

---

## Deeper recall + persistence (v0.9)

v0.8 remembered in-memory with raw token overlap. v0.9 deepens it by lifting the search core from the author's own [seCall](https://github.com/hang-in/seCall) engine — all behind a `friend-engine` Cargo feature (off by default, so the normal build pulls no extra deps and the golden output is untouched):

- **Korean morphology** (Stage 0): recall tokenizes with Lindera (ko-dic), stripping 조사/어미 — so "비가 온다" now matches a past "비 온다" line that whitespace overlap missed.
- **SQLite + FTS5 BM25** (Stage 1): the store becomes real SQLite with an FTS5 index; recall ranks past lines by BM25 (term frequency / rarity / length) with OR-match, instead of a raw token count.
- **Cross-session memory**: `--chat` persists to `~/.local/share/tunaSalon/memory.db` (`$SALON_MEMORY_DB` overrides), so personas remember across runs. Tests stay in `:memory:` and write nothing to disk.

Next (v0.10): BGE-M3 semantic search + HNSW + hybrid fusion. Alongside v0.9 the `--chat` room got livelier — a tuned 3-way config (cross-excitation + no same-speaker-twice) and `/topic` to steer the conversation (gated by `smoke_chat`).

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
cargo run --features friend-engine -- --chat      # personas remember across sessions (morphology + SQLite BM25 recall)
cargo run --example persona_collapse              # same model, two personas — does it hold? (needs Ollama)
cargo run --example mixed_bench                   # cloud + friend vLLM in the same room (needs both backends)
cargo run --example chat_demo                     # non-interactive chat loop with flow readout per line
cargo test                                        # 222 tests (230 with --features friend-engine)
```

Knobs: **μ** (per-persona chattiness) · **θ** (silence threshold) · **k** (RRF tie-break sharpness) · **β** (urge recovery speed). Same `--seed` gives identical output every run, so it's verifiable headless.

---

## Status

**v0.9 (now):** friend engine, deeper — Korean morphology (Lindera) + SQLite/FTS5 BM25 recall + cross-session persistence, lifted from the author's seCall engine and gated behind a `friend-engine` feature (golden untouched). Plus a livelier `--chat` (tuned 3-way + `/topic`). Rust, 222 tests (230 with the feature), smoke gates green.

**So far:**
- **v0.1 — rhythm:** speech/silence rhythm from μ, θ, and the tie-break alone.
- **v0.2 — chemistry (α):** who riles up whom; room presets (calm / pub / argument / chaos) and persona pairings.
- **v0.3 — local LLMs:** Ollama personas generate actual lines. Engine decides who speaks; LLM fills in content. `persona_collapse` example: same model, different persona prompts.
- **v0.4 — concurrent / mixed-model:** backend pool (Ollama + OpenAI-compatible), per-persona routing, concurrency caps, fallback. `mixed_bench` example.
- **v0.5 — join the room:** human-in-the-loop chat (HumanChannel + `--chat` TUI). `chat_demo` example.
- **v0.6 — FlowMeter:** convergence/divergence measurement (observe-only). Token-overlap approximation; BGE-M3 embeddings later. Live gauge in TUI + `chat_demo` readout.
- **v0.7 — MetaController:** macro→micro feedback (cool the room as it converges). The original engine-layer roadmap — rhythm → chemistry → LLM → concurrency → chat → flow-meter → meta-controller — is now complete.
- **v0.8 — friend engine (first increment):** participation-based memory + keyword recall + recall-eval harness. Personas start remembering what was said in rooms they were in.
- **v0.9 — friend engine, deeper:** Korean morphology + SQLite/FTS5 BM25 recall + cross-session persistence (feature-gated). Livelier `--chat` (3-way config + `/topic`).

**What's next (separate tracks, no fixed order):**
- **v0.10 — semantic recall:** BGE-M3 embeddings (ONNX) + HNSW (usearch) + hybrid BM25/vector fusion, on top of the v0.9 store. Then forgetting, subjective storage, cross-room impressions.
- **Web frontend:** moving the chat UI to the browser for a production-grade, shareable app (Rust engine kept as-is, served over WebSocket; the TUI stays as a debug tool). Planned — see `docs/plans/salon-web-frontend.md`.
- **Persona synthesis + characters:** building personas from MBTI / blood-type / zodiac / role presets, with pixel-art (Cyworld-minimi-style) avatars for the web — see `docs/temp/salon-persona-ui.md`.

---

## Why

Most multi-agent LLM demos solve a task and stop. Real small talk has no task and no end — it just flows and fades. That needs a different kind of engine, so here's one you can poke at.

---

*Sources: Multivariate Hawkes process (self-exciting point process), Reciprocal Rank Fusion, AutoGen GroupChat pattern. Full design notes in [docs/reference/salon-engine-design.md](docs/reference/salon-engine-design.md).*
