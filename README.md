**English** · [한국어](README.ko.md)

# tunaSalon

![Rust](https://img.shields.io/badge/Rust-2021-CE422B?logo=rust&logoColor=white)
![status](https://img.shields.io/badge/status-v0.2-blue)
![tests](https://img.shields.io/badge/tests-50%20passing-brightgreen)
![no LLM yet](https://img.shields.io/badge/v0.1--v0.2-no%20LLM%20yet-8A2BE2)
![determinism](https://img.shields.io/badge/output-deterministic-informational)

A terminal app that drops local-LLM personas into a room and lets them small-talk. The catch: the star isn't the personas — it's the **conversation-flow engine** that decides who speaks when, and when the room just goes quiet.

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

## Try it

All you need is [Rust](https://rustup.rs). v0.1–v0.2 need no LLM and no network.

```bash
cargo run                                    # watch the meter live (TUI). q to quit, space to pause
cargo run -- --headless --ticks 200          # deterministic NDJSON, one line per tick
cargo run -- --sweep                         # θ × k grid + preset comparison
cargo run -- --room argument                 # room mood preset (calm/pub/argument/chaos)
cargo run -- --room chaos --fsm              # chemistry + no speaker twice in a row
cargo run -- --theta 0.7 --k 5 --beta 0.4    # turn the knobs
cargo test                                   # 50 tests, including the smoke gates
```

Knobs: **μ** (per-persona chattiness) · **θ** (silence threshold) · **k** (RRF tie-break sharpness) · **β** (urge recovery speed). Same `--seed` gives identical output every run, so it's verifiable headless.

---

## Status

**v0.2 (now):** the rhythm engine plus chemistry. Still no LLM — fake utterances let you check the *timing* and the *chemistry* are alive: cross-excitation (α), room presets, persona modifiers, and FSM transitions. Deterministic, with a debug meter. Rust, 50 tests, smoke gates green.

**So far:**
- **v0.1 — rhythm:** speech/silence rhythm from μ, θ, and the tie-break alone.
- **v0.2 — chemistry (α):** who riles up whom; room presets (calm / pub / argument / chaos) and persona pairings.

**Next:**
- **v0.3 — local LLMs:** Ollama personas generate the actual lines. The engine still decides who speaks; the model only fills in content. Watch whether a small model holds its persona (persona collapse).

---

## Why

Most multi-agent LLM demos solve a task and stop. Real small talk has no task and no end — it just flows and fades. That needs a different kind of engine, so here's one you can poke at.

---

*Sources: Multivariate Hawkes process (self-exciting point process), Reciprocal Rank Fusion, AutoGen GroupChat pattern. Full design notes in [docs/reference/salon-engine-design.md](docs/reference/salon-engine-design.md).*
