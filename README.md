**English** · [한국어](README.ko.md)

# tunaSalon

![Rust](https://img.shields.io/badge/Rust-2021-CE422B?logo=rust&logoColor=white)
![status](https://img.shields.io/badge/status-v0.10%20%2B%20web-blue)
![tests](https://img.shields.io/badge/tests-291%20passing-brightgreen)
![LLM optional](https://img.shields.io/badge/LLM-optional%2C%20default--off-8A2BE2)
![determinism](https://img.shields.io/badge/output-deterministic-informational)

A browser app where you open a **topic debate room** and LLM personas argue it out — they remember, rebut, and call each other by name. The real star, though, isn't the personas: it's the **conversation-flow engine** underneath that decides who speaks when, and when the room just falls silent. On top of it sits a *producer* that turns each topic into a kind of argument — a courtroom, a policy duel, a moral dilemma — and you can jump in as a first-class participant whenever you like. (It started as a terminal toy for the engine alone; you can still run it that way — `cargo run` opens the live meter.)

Designing speech is easy. Designing silence is hard. This project is a little backwards on purpose.

> Most of this README walks the engine, layer by layer (v0.1–v0.10). For the product it now powers, jump to [Topic debate in the browser](#topic-debate-in-the-browser).

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

## How the engine got here (v0.3 → v0.10)

Local LLMs, mixed-model backends, human-in-the-loop chat, the FlowMeter and MetaController, then a friend engine that grows from keyword recall to morphology + BM25 to in-process semantic search. Each step is small, runnable on its own, and keeps the default build deterministic and golden-clean.

**→ Full walkthrough, with real output per layer: [HISTORY.md](HISTORY.md).**

---

## Topic debate in the browser

The engine track (v0.1–v0.10) was always *means to an end*: liveliness. The product is the room — and the room has become a **topic-based LLM debate app** you open in a browser (axum WebSocket + React, `--features "web redis-bus"`).

A debate of competent essays is boring. What makes it fun is a **producer** layer — and it lives in its own framework-independent module, `src/debate/`, so the engine files don't bloat:

- **DebatePlan** — a topic is mapped, deterministically (no LLM meta-call), to a debate **mode**: `Courtroom`, `PolicyDuel`, `MoralDilemma`, `PersonalStakes`, `Forecasting`, `DesignReview`. "AI 판사가 공정할까?" becomes a courtroom; "AI 규제와 오픈소스" a policy duel. The mode anchors the room so it stops drifting off-topic.
- **Format variation** — instead of every turn being the same length, a per-turn hint cycles through formats: cross-examine, steelman-then-attack, one concrete case, a measurable threshold, a conditional concession. Some are deliberately one or two sentences, so turns stop feeling uniform.
- **Loop-breaking twist cards** — when the room converges (FlowMeter) or a speaker repeats, the producer injects a fresh "new development" (a wrongful-conviction stat, a maintainer who quit over regulation) and forces the participants to react to it.

All of it is injected only into the generation snapshot, never the stored history — so the deterministic LLM-off golden path stays byte-identical.

**The room, as a product:**

- **Create a room two ways** — "만들기" seeds a **deterministic random trio** from the room id (same topic → same three), or "직접 고르기" lets you **build 2–3 participants** by axes (blood-type / MBTI / zodiac / role) with a live nickname preview before entering.
- **Invite & remove** mid-debate; participants route to an available backend (a cloud model, or a friend's vLLM when it's up — never to a dead one).
- **Readable chat** — messages render light Markdown, and each participant's nickname is highlighted in that persona's colour when others address them.
- **Lobby with persistence** — rooms checkpoint to `rooms.db` and restore with their participants, topics, and history.
- **A speaking gauge** per participant: the Hawkes urge λ as a bar against the θ threshold — cross it and you're *eligible* to speak (the RRF tie-break still picks one), so you watch the room's rise-speak-suppress-quiet rhythm directly.

**Redis multi-session** (`redis-bus` feature, opt-in via `SALON_REDIS_URL`) is a *volatile coordination* layer — room command/event streams, owner lease, presence — not a memory store. `LiveSession` stays the single writer per room and SQLite (`memory.db`, `rooms.db`) stays the durable source of truth; flushing Redis never loses history.

The whole product track sits behind the `web` / `redis-bus` features. The default build and the headless golden output are untouched.

---

## Try it

All you need is [Rust](https://rustup.rs). The default run needs no LLM and no network.

```bash
cargo run                                         # watch the meter live (TUI). q to quit, space to pause
cargo run -- --chat                               # join the room: interactive chat (needs a real terminal)
cargo run --features "web redis-bus" -- --web --topic "AI 판사가 공정할까?"  # browser debate app (topic modes, room builder, invite)
cargo run -- --headless --ticks 200               # deterministic NDJSON, one line per tick
cargo run -- --sweep                              # θ × k grid + preset comparison
cargo run -- --room argument                      # room mood preset (calm/pub/argument/chaos)
cargo run -- --room chaos --fsm                   # chemistry + no speaker twice in a row
cargo run -- --theta 0.7 --k 5 --beta 0.4         # turn the knobs
cargo run -- --llm                                # opt in to LLM (default cloud model, needs network)
cargo run --features friend-engine -- --chat      # personas remember across sessions (morphology + SQLite BM25 recall)
cargo run --features friend-engine-semantic -- --chat  # + semantic recall (loads BGE-M3 if the model is present)
cargo run --example persona_collapse              # same model, two personas — does it hold? (needs Ollama)
cargo run --example mixed_bench                   # cloud + friend vLLM in the same room (needs both backends)
cargo run --example chat_demo                     # non-interactive chat loop with flow readout per line
cargo test                                        # 291 tests (297 with friend-engine, 326 with web+redis-bus)
```

Knobs: **μ** (per-persona chattiness) · **θ** (silence threshold) · **k** (RRF tie-break sharpness) · **β** (urge recovery speed). Same `--seed` gives identical output every run, so it's verifiable headless.

---

## Status

**Now:** a **topic-debate web app** runs on top of the complete v0.1–v0.10 engine. Browser debate over WebSocket with a framework-independent producer layer (deterministic debate modes, format variation, loop-breaking twist cards), random-or-build-your-own room creation, dynamic invite, Markdown chat with nickname highlighting, lobby persistence, and a Redis multi-session bus — all behind the `web` / `redis-bus` features, with the default build and headless golden output untouched. The engine track peaks at v0.10: semantic recall (BGE-M3 ONNX in-process + HNSW + hybrid BM25/vector RRF) behind `friend-engine-semantic`. Rust, 291 tests (297 with friend-engine, 326 with web+redis-bus), smoke gates green.

**The path here:** rhythm (v0.1) → chemistry → local LLMs → mixed-model → join the room → FlowMeter → MetaController → a friend engine that remembers (v0.8–v0.10) → the debate app. Each layer, with real output, is in **[HISTORY.md](HISTORY.md)**.

**What's next (separate tracks, no fixed order):**
- **debate producer, further:** hidden per-persona goals (public stance + private objective), topic-specific evidence cards, an argument ledger of unanswered questions.
- **friend engine, further:** forgetting, subjective per-persona storage, cross-room impressions of people, on top of the v0.10 store.
- **Web app:** pixel-art (Cyworld-minimi-style) avatars, multi-room, user profiles/presets, web search (tool use). Persona axes assembled from blood-type / MBTI / zodiac / role with auto Indian-style names — see `docs/temp/salon-persona-ui.md`.

---

## Why

Point a few LLMs at a topic and they'll all answer, every turn, forever — a panel that never interrupts, never holds back, never gets bored. That isn't how an argument works. A real one has *timing*: someone jumps in, someone bites their tongue, the room circles the same point until somebody drags in a new angle. None of that is in the prompt — it's rhythm.

So tunaSalon builds the rhythm first — whose urge to speak is rising, who just spoke, when the room should be allowed to fall silent — and lets the personalities, and the fight, fall out of it. The models only supply the words. The interesting part is deciding who gets to say them, and when to let the silence sit.

---

*Sources: Multivariate Hawkes process (self-exciting point process), Reciprocal Rank Fusion, AutoGen GroupChat pattern. Full design notes in [docs/reference/salon-engine-design.md](docs/reference/salon-engine-design.md).*
