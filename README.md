**English** · [한국어](README.ko.md)

# tunaSalon

![Rust](https://img.shields.io/badge/Rust-2021-CE422B?logo=rust&logoColor=white)
![status](https://img.shields.io/badge/status-v0.10%20%2B%20web-blue)
![tests](https://img.shields.io/badge/tests-306%20passing-brightgreen)
![LLM optional](https://img.shields.io/badge/LLM-optional%2C%20default--off-8A2BE2)
![determinism](https://img.shields.io/badge/output-deterministic-informational)

Throw a topic into a room and watch LLM personas bicker it out: a courtroom showdown, a mint-chocolate war, whatever. They remember, they rebut, they call each other by nickname. And right when it heats up, you can slip in and the whole room turns to react to you.

The real star, though, isn't the personas. tunaSalon's heart is the conversation-flow engine that decides **who opens their mouth when, and when everyone just goes quiet**. The personas are only voices riding on top of it. (It started life as a terminal toy for that engine alone. You can still summon those days with one line: `cargo run`.)

> Making something speak is easy. The hard part is letting it fall silent naturally. This project goes backwards on purpose: it designs the silence first.

---

## How is this different from just persona chat?

The usual "give an LLM a persona and chat" setup answers in turns, or whoever got prompted, or everyone-once-per-round. No timing, no silence, no "I'll just listen this round." A panel that never learns to interrupt.

In tunaSalon each persona has an **urge to speak** that rises and falls over time. If nobody's urge is high enough, the room goes quiet. If two want in at once, a tie-break picks one. Conversations heat up and cool down on their own.

You don't script the dialogue rules. You turn a few knobs and **watch**, and personalities fall out as rhythms: the chatterbox, the quiet one, the one who only chimes in sometimes.

---

## How the engine works, very briefly

Four small parts.

| knob | what it is |
|---|---|
| `μ` chattiness | a persona's baseline urge to talk (0-1) |
| `λ` urge | recovers toward μ each tick, drops right after they speak (so nobody hogs the floor) |
| `θ` silence gate | each tick, if no λ clears this line, the room stays quiet. Higher means more silence |
| `RRF` who speaks | when several clear θ, fuse three signals to pick one: who wants it most (λ) + who's spoken least lately (fairness) + a pinch of randomness |

The loop, every tick:

```
nudge each λ toward μ  ->  gate: if nobody clears θ, stay silent
                       ->  else pick one speaker via RRF
                       ->  drop that speaker's λ  ->  repeat
```

No LLM needed to get a rhythm: a fake one-word utterance is enough. The personality lives in the **timing**.

### What the knobs do (real output)

`cargo run -- --sweep` runs θ at a fixed seed. μ = friend 0.80, chaos 0.70, summarizer 0.25:

```
θ=0.40  silence   0   friend 100  chaos 100  summarizer 0   # gate loose -> everyone passes, nonstop ping-pong
θ=0.65  silence 100   friend  62  chaos  38  summarizer 0   # gate bites -> speech/silence rhythm, μ shows through
θ=0.78  silence 171   friend  29  chaos   0  summarizer 0   # gate harsh -> only the chattiest gets a word in
```

Same μ, but a single θ turns the room from nonstop, to rhythmic, to nearly silent.

### Chemistry (α)

v0.2 adds **cross-excitation α**: one persona speaking lifts the others' urge. Who riles up whom *is* the room's chemistry, and you pick the mood with a preset.

```
preset=Calm      silence 99   friend 67  chaos 34  summarizer  0   # weak α -> the quiet one stays quiet
preset=Argument  silence  0   friend 76  chaos 76  summarizer 48   # strong α -> cross-excitation drags the quiet one in
```

The same summarizer (μ=0.25) never speaks in Calm but speaks 48 times in Argument. The personas are identical: the **room's mood pulls the quiet one into the conversation**.

### The meter

`cargo run` opens the TUI. The λ bars drift toward and away from the θ line (`|`), and you see who spoke and why, live:

```
┌events──────────────────────────────────────┐┌gauges────────────────────┐
│t8 (silence)                                ││Chaos Guest               │
│t9 (silence)                                ││########|.... 0.63        │
│t10 Friendly Regular                        ││########|.... 0.67        │
│                                            ││Quiet Summarizer          │
│                                            ││###.....|.... 0.25        │
│                                            ││speak 11  silence 6       │
└────────────────────────────────────────────┘└──────────────────────────┘
```

You can't debug an emergent system without a meter. You tune by watching the numbers: why the room went quiet, why one persona is hogging the floor.

---

## Topic debate in the browser

The engine track (v0.1-v0.10) was always *means to an end*: liveliness. The product is the room, and the room has become a **topic-based LLM debate app** you open in a browser (axum WebSocket + React, `--features "web redis-bus"`).

A debate of competent essays is boring. The fun comes from a **producer** layer, and it lives in its own framework-independent module `src/debate/` so the engine files don't bloat:

- **DebatePlan** - a topic maps, deterministically (no LLM meta-call), to a debate **mode**: `Courtroom`, `PolicyDuel`, `MoralDilemma`, `PersonalStakes`, `Forecasting`, `DesignReview`, plus a `CasualBanter` mode for light topics (mint-choc, pineapple-on-pizza). "AI 판사가 공정할까?" becomes a courtroom; "AI 규제와 오픈소스" a policy duel. The mode anchors the room so it stops drifting off-topic.
- **Format variation** - instead of every turn being the same length, a per-turn hint cycles through formats: cross-examine, steelman-then-attack, one concrete case, a measurable threshold, a conditional concession. Some are deliberately one or two sentences, so turns stop feeling uniform.
- **Loop-breaking twist cards** - when the room converges (FlowMeter) or a speaker repeats, the producer injects a fresh development (a wrongful-conviction stat, a maintainer who quit over regulation) and forces a reaction.
- **Friends, not a courtroom** - even serious topics keep a casual register: "argue like friends over dinner, not lawyers in a courtroom." The analytical focus (evidence, premises, trade-offs) stays; only the register softens.

All of it is injected only into the generation snapshot, never the stored history, so the deterministic LLM-off golden path stays byte-identical.

**What you'll meet in the room:**

- **Personas with personality** - assembled from blood-type / MBTI / zodiac (four axes). These set not just *who* a persona is (a trait description) but *how* they talk (a voice directive). An ENTP cuts in with "wait, but-" and a flipped question; a blood-type-A hedges politely; a Pisces answers in metaphor. The same model splits into distinct voices.
- **Two ways to make a room** - "만들기" seeds a **deterministic random trio** from the room id (same topic, same three); "직접 고르기" lets you **build 2-3 participants** by axes with a live nickname preview before you enter.
- **You're a first-class participant** - jump in and every persona's urge spikes; for a few turns your message becomes the top priority; call a nickname and that persona answers next. Toss a line into a finished debate and it reopens.
- **A closing report** - when a debate ends, a neutral analyst writes a conclusion-first Markdown report: the verdict, each participant's stance (yours included), points of agreement, and what stayed unresolved.
- **Read-only sharing** - a debate that ended well can be shared by an opaque token link. Whoever opens it reads the full transcript (avatars, colours, conclusion) with no login, on desktop or mobile. Only rooms you explicitly share become public.
- **An archive of past debates** - every room is kept. Pick one from the list (with its start and conclusion dates) and re-enter to restore participants, topics, and history.
- **A speaking gauge** per participant: the Hawkes urge λ as a bar against the θ threshold. Cross it and you're *eligible* to speak (the RRF tie-break still picks one), so you watch the rise-speak-suppress-quiet rhythm directly.

**Redis multi-session** (`redis-bus` feature, opt-in via `SALON_REDIS_URL`) is a *volatile coordination* layer: room command/event streams, owner lease, presence. Not a memory store. `LiveSession` stays the single writer per room and SQLite (`memory.db`, `rooms.db`) stays the durable source of truth; flushing Redis never loses history.

The whole product track sits behind the `web` / `redis-bus` features. The default build and the headless golden output are untouched.

---

## How the engine got here (v0.3 -> v0.10)

Local LLMs, mixed-model backends, human-in-the-loop chat, the FlowMeter and MetaController, then a friend engine that grows from keyword recall to morphology + BM25 to in-process semantic search. Each step is small, runnable on its own, and keeps the default build deterministic and golden-clean.

**-> Full walkthrough, with real output per layer: [HISTORY.md](HISTORY.md).**

---

## Try it

All you need is [Rust](https://rustup.rs). The default run needs no LLM and no network.

```bash
cargo run                                         # watch the meter live (TUI). q to quit, space to pause
cargo run -- --chat                               # join the room: interactive chat (needs a real terminal)
cargo run --features "web redis-bus" -- --web --topic "AI 판사가 공정할까?"  # browser debate app
cargo run -- --headless --ticks 200               # deterministic NDJSON, one line per tick
cargo run -- --sweep                              # θ × k grid + preset comparison
cargo run -- --room argument                      # room mood preset (calm/pub/argument/chaos)
cargo run -- --room chaos --fsm                   # chemistry + no speaker twice in a row
cargo run -- --theta 0.7 --k 5 --beta 0.4         # turn the knobs
cargo run -- --llm                                # opt in to LLM (default cloud model, needs network)
cargo run --features friend-engine -- --chat      # personas remember across sessions (morphology + SQLite BM25 recall)
cargo run --features friend-engine-semantic -- --chat  # + semantic recall (loads BGE-M3 if the model is present)
cargo run --example persona_collapse              # same model, two personas - does it hold? (needs Ollama)
cargo run --example mixed_bench                   # cloud + friend vLLM in the same room (needs both backends)
cargo run --example chat_demo                     # non-interactive chat loop with flow readout per line
cargo test                                        # 306 tests (friend-engine 319 / semantic 347 / web 356)
```

| flag | what it is |
|---|---|
| `--theta` | silence threshold |
| `--k` | softens RRF's top-pick bias |
| `--beta` | urge recovery / decay speed |
| `--room` | room mood preset |

Same `--seed` gives byte-identical output every run, so it's verifiable headless.

---

## Status

**Now:** a **topic-debate web app** runs on top of the complete v0.1-v0.10 engine. Browser debate over WebSocket with a framework-independent producer layer (deterministic debate modes, format variation, loop-breaking twist cards, a friendly register), personas with four-axis personality and voice, random-or-build-your-own room creation, dynamic invite, first-class human participation, closing reports, read-only share links, an archive of past debates, lobby persistence, and a Redis multi-session bus. All behind the `web` / `redis-bus` features, with the default build and golden output untouched. The engine track peaks at v0.10: semantic recall (BGE-M3 ONNX in-process + HNSW + hybrid BM25/vector RRF) behind `friend-engine-semantic`. Rust, 306 tests (319 with friend-engine, 347 with semantic, 356 with web), smoke gates green.

**The path here:** rhythm (v0.1) -> chemistry -> local LLMs -> mixed-model -> join the room -> FlowMeter -> MetaController -> a friend engine that remembers (v0.8-v0.10) -> the debate app. Each layer, with real output, is in **[HISTORY.md](HISTORY.md)**.

**What's next (separate tracks, no fixed order):**
- **debate producer, further:** hidden per-persona goals (public stance + private objective), topic-specific evidence cards, an argument ledger of unanswered questions.
- **friend engine, further:** forgetting, subjective per-persona storage, cross-room impressions of people, on top of the v0.10 store.
- **web app:** pixel-art (Cyworld-minimi-style) avatars, multi-room, user profiles/presets, web search (tool use).

---

## Why

Point a few LLMs at a topic and they'll all answer, every turn, forever: a panel that never interrupts, never holds back, never gets bored. That isn't how an argument works. A real one has *timing*: someone jumps in, someone bites their tongue, the room circles the same point until somebody drags in a new angle. None of that is in the prompt. It's rhythm.

So tunaSalon builds the rhythm first: whose urge to speak is rising, who just spoke, when the room should be allowed to fall silent. Then it lets the personalities, and the fight, fall out of it. The models only supply the words. The interesting part is deciding who gets to say them, and when to let the silence sit.

---

*Sources: Multivariate Hawkes process (self-exciting point process), Reciprocal Rank Fusion, AutoGen GroupChat pattern. Full design notes in [docs/reference/salon-engine-design.md](docs/reference/salon-engine-design.md).*
