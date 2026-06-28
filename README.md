**English** · [한국어](README.ko.md)

# tunaSalon

![Rust](https://img.shields.io/badge/Rust-2021-CE422B?logo=rust&logoColor=white)
![status](https://img.shields.io/badge/status-v0.10%20%2B%20web-blue)
![tests](https://img.shields.io/badge/tests-306%20passing-brightgreen)
![LLM optional](https://img.shields.io/badge/LLM-optional%2C%20default--off-8A2BE2)
![determinism](https://img.shields.io/badge/output-deterministic-informational)

**tunaSalon** is a browser debate app where several LLM personas take a topic and talk, cut in, rebut, and sometimes just listen.

The usual persona chat has everyone answer in turn, or one line each per round. tunaSalon doesn't work that way. Each persona has its own **urge to speak**, and only speaks when that urge has risen high enough. If nobody is warmed up enough to talk, the room goes quiet.

> Making something speak is easy. The hard part is letting it fall silent naturally.

The heart of tunaSalon isn't "what to generate" so much as **who should be talking right now**. The LLM fills in the sentences; the engine decides the timing.

---

## What's different

A typical group-chat LLM app usually works one of these ways:

- one persona answers at a time, in a fixed order,
- only the persona you addressed answers,
- every persona says a line every turn.

These are easy to build but rarely look like a debate. A real conversation has silence, interruption, hesitation, anti-monopoly, and sudden re-ignition.

tunaSalon builds the **rhythm of the conversation** before it builds the content.

| typical persona chat | tunaSalon |
|---|---|
| speaks by turn order or by being called | speaks by urge to talk |
| someone always speaks each turn | silence when nobody is ready |
| everyone answers often | exactly one speaker is picked at a time |
| personas differ mostly by prompt | personas differ by voice + speaking rhythm |
| flow depends on the prompt | a flow engine controls the timing |

So even with the same model, the personas move differently: the chatterbox cuts in often, the quiet one listens a while and opens up only in a certain mood.

---

## What you can do now

tunaSalon runs as two tracks.

1. **Engine track**
   A deterministic conversation-flow simulator that runs without any LLM. `cargo run` opens the TUI meter; `--headless` gives byte-identical NDJSON output you can verify.

2. **Web app track**
   A browser debate app behind the `web` / `redis-bus` features. Built on axum WebSocket + React: from typing a topic, through persona debate, human participation, a closing report, all the way to a share link.

### Key features

| feature | what it does |
|---|---|
| topic-based debate | type a topic and a room opens; the personas start arguing |
| deterministic debate modes | maps a topic to `Courtroom`, `PolicyDuel`, `MoralDilemma`, `Forecasting`, `DesignReview`, `CasualBanter`, and so on |
| personas with personality | assembles trait and voice from four axes: blood-type, MBTI, zodiac |
| random / manual room creation | a deterministic random trio from the room id, or 2-3 participants you build yourself |
| first-class human participation | jump in and the personas react around your line; call a nickname and that persona answers first |
| speaking gauge | see how far each participant's urge `λ` has risen against the silence gate `θ` |
| closing report | after a debate ends, a Markdown summary of the conclusion, each stance, agreements, and what stayed split |
| read-only sharing | only rooms you share are exposed by a token link; anyone reads the transcript with no login |
| archive of past debates | restore a past room's topic, participants, history, and conclusion |
| Redis multi-session | command/event streams, owner lease, and presence so several browsers/instances share a room reliably |

---

## The engine in brief

tunaSalon's conversation flow is controlled by four values.

| value | meaning |
|---|---|
| `μ` | baseline chattiness: how talkative a persona is by default |
| `λ` | current urge: recovers toward `μ` over time, drops after speaking |
| `θ` | silence gate: if no `λ` clears it, the room stays quiet |
| `RRF` | speaker choice: when several earn the right to speak, picks one actual speaker |

The loop, every tick, is simple:

```text
recover each λ toward μ
-> check who cleared θ
-> nobody cleared it: stay silent
-> several cleared it: pick one via RRF
-> drop the chosen speaker's λ
-> repeat
```

This is what lets tunaSalon be a debate floor where whoever wants to talk rises in turn and the room quiets when it cools down, rather than a panel where everyone speaks every time.

---

## Turning the knobs

`cargo run -- --sweep` runs the silence gate `θ` at a fixed seed. The example below fixes `friend μ=0.80`, `chaos μ=0.70`, `summarizer μ=0.25`:

```text
θ=0.40  silence   0   friend 100  chaos 100  summarizer 0
θ=0.65  silence 100   friend  62  chaos  38  summarizer 0
θ=0.78  silence 171   friend  29  chaos   0  summarizer 0
```

`μ` is unchanged, but a single `θ` turns the room into:

| `θ` | result |
|---|---|
| low | talks almost nonstop |
| mid | a rhythm of speech and silence appears |
| high | only the chattiest persona barely gets a word in |

---

## Chemistry: cross-excitation `α`

From v0.2, **cross-excitation `α`** between personas comes in. When one speaks, the others' urge rises. Who riles up whom, and how much, becomes the room's chemistry.

```text
preset=Calm      silence 99   friend 67  chaos 34  summarizer  0
preset=Argument  silence  0   friend 76  chaos 76  summarizer 48
```

The same `summarizer (μ=0.25)` never speaks in the calm room and cuts in 48 times in the argumentative one. The personas are identical; the room's mood drew the quiet one out.

---

## The TUI meter

`cargo run` opens the TUI, where each persona's urge to speak is live. The bar is `λ`, the vertical line is the silence gate `θ`.

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

You tune by reading the numbers: why the room went quiet, why one persona is hogging the floor, which value is set too high.

---

## The browser debate app

The web app is a product track on top of the engine.

```bash
cargo run --features "web redis-bus" -- --web --topic "AI 판사가 공정할까?"
```

In the browser you enter a topic, make a room, read the persona debate, and jump in yourself.

### DebatePlan

The producer layer in `src/debate/` maps a topic to a debate mode deterministically. It doesn't pick the format with an LLM meta-call; it selects the mode by reproducible rules.

| mode | example topic |
|---|---|
| `Courtroom` | Will an AI judge be fair? |
| `PolicyDuel` | AI regulation vs. open source |
| `MoralDilemma` | Is surveillance acceptable for convenience? |
| `PersonalStakes` | How far should a family's choice be respected? |
| `Forecasting` | How will developers change in five years? |
| `DesignReview` | Is this architecture maintainable? |
| `CasualBanter` | Is mint-choc food, or toothpaste? |

### Format variation

So that every turn doesn't become a same-length essay, the speaking format varies.

- cross-examination
- steelman then rebut
- one concrete case
- propose a measurable threshold
- conditional concession
- a short one or two-sentence reaction

When the debate circles the same spot, a twist card adds a fresh development: a wrongful-conviction stat, a regulatory burden, a maintainer who quit, a cost increase, to draw out a reaction again.

### A friends-arguing tone

The analytical focus stays, but the register is dialed down from stiff. Less like sentences in a courtroom, more like friends going "wait, isn't that a bit different though?"

---

## Redis multi-session

Turning on the `redis-bus` feature uses Redis as a multi-session coordination layer.

```bash
SALON_REDIS_URL=redis://127.0.0.1:6379 \
cargo run --features "web redis-bus" -- --web
```

What Redis handles:

| role | description |
|---|---|
| command stream | relays commands coming into a room |
| event stream | propagates events raised in a room |
| owner lease | keeps a single writer per room |
| presence | tracks who's connected |

Redis is not a memory store. The source of truth for durable data is SQLite.

| store | role |
|---|---|
| `memory.db` | memory, recall, friend-engine data |
| `rooms.db` | rooms, participants, history, share info |
| Redis | volatile multi-session coordination |

Flushing Redis never loses history.

---

## Running it

With just [Rust](https://rustup.rs) you can do the default run. The default path needs no LLM and no network.

```bash
cargo run
```

### Main commands

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

### Command notes

| command | description |
|---|---|
| `cargo run` | run the TUI meter. `q` to quit, `space` to pause. |
| `cargo run -- --chat` | interactive room in the terminal. needs a real terminal. |
| `cargo run --features "web redis-bus" -- --web` | run the browser debate app. |
| `cargo run -- --headless --ticks 200` | deterministic NDJSON, one line per tick. |
| `cargo run -- --sweep` | `θ × k` grid and room-preset comparison. |
| `cargo run -- --room argument` | run the argumentative room preset. |
| `cargo run -- --room chaos --fsm` | chemistry + no-same-speaker-twice FSM. |
| `cargo run -- --llm` | LLM opt-in. off by default. |
| `cargo run --features friend-engine -- --chat` | recall via morphology + SQLite BM25. |
| `cargo run --features friend-engine-semantic -- --chat` | + semantic recall via BGE-M3. |
| `cargo test` | run the tests. |

### Options

| option | meaning |
|---|---|
| `--theta` | silence threshold |
| `--k` | softens RRF's top-pick bias |
| `--beta` | urge recovery / decay speed |
| `--room` | room mood preset |
| `--seed` | seed for deterministic output |

The same `--seed` gives byte-identical output every run. Headless output uses this property for automatic verification.

---

## Feature flags

The product track and the experimental track sit behind features. As a rule, the default build and the headless golden output are kept untouched.

| feature | description |
|---|---|
| `web` | axum WebSocket + React browser debate app |
| `redis-bus` | Redis-based multi-session coordination |
| `friend-engine` | recall via morphology + SQLite BM25 |
| `friend-engine-semantic` | semantic recall: BGE-M3 ONNX + HNSW + BM25/vector RRF |

---

## Status

tunaSalon currently runs as a browser debate app on top of the v0.1-v0.10 engine.

| area | status |
|---|---|
| engine | Hawkes-based urge, silence gate, RRF speaker choice, chemistry presets |
| TUI | speaking gauge, event log, headless deterministic output |
| LLM | opt-in, off by default, mixed local/cloud backends |
| friend engine | morphology + BM25 recall, optional semantic recall |
| web app | WebSocket debate, persona building, human participation, closing report, share links, archive |
| multi-session | Redis command/event stream, owner lease, presence |
| tests | 306 passing by default, 319 friend-engine, 347 semantic, 356 web |

The detailed per-version walkthrough, with real output, is in [HISTORY.md](HISTORY.md).

---

## Roadmap

The next work runs as separate tracks. The order is not fixed.

### Producer layer

- hidden per-persona goals
- splitting a public stance from a private objective
- topic-specific evidence cards with real data
- a ledger of unanswered questions
- stronger detection of and intervention on repeated points

### friend engine

- forgetting
- subjective per-persona storage
- cross-room impressions of people
- separating long-term memory from per-room memory

### web app

- pixel-art avatars
- multi-room
- user profiles / presets
- web search (tool use)
- a better share page

---

## Why

Point several LLMs at one topic and mostly everyone answers, every time. That's closer to a bundle of answers than a debate.

A real conversation has timing.

- someone cuts in.
- someone just listens.
- if one person talks too long, the flow dies.
- the room goes quiet, then a single line brings it back.

tunaSalon designs this timing first. The personas are voices riding on top of it.

There are plenty of models that generate good sentences. The problem tunaSalon takes on is a little different.

> In this room, right now, who should speak?
> And when is it fine for nobody to?

---

## References

- [HISTORY.md](HISTORY.md) - walkthrough of the v0.1-v0.10 engine and the product track
- [docs/reference/salon-engine-design.md](docs/reference/salon-engine-design.md) - engine design in detail
- Multivariate Hawkes process - the self-exciting point process behind the urge-to-speak model
- Reciprocal Rank Fusion - speaker choice fusing several signals
- AutoGen GroupChat pattern - reference for the LLM group-chat pattern

---

## License

See the `LICENSE` file in the repository for license information.
