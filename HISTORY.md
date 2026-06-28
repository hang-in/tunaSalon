# tunaSalon - build history

How the engine was built, one layer at a time. Each layer is a small, self-contained step you can run on its own. For the short version and the live product, see the [README](README.md).

*(v0.1 rhythm and v0.2 chemistry are walked through in the README's "How it works". The details below pick up at v0.3.)*

**At a glance:**
- **v0.1 - rhythm:** speech/silence rhythm from μ, θ, and the tie-break alone.
- **v0.2 - chemistry (α):** who riles up whom; room presets (calm / pub / argument / chaos) and persona pairings.
- **v0.3 - local LLMs:** Ollama personas generate actual lines. Engine decides who speaks; LLM fills in content.
- **v0.4 - concurrent / mixed-model:** backend pool (Ollama + OpenAI-compatible), per-persona routing, concurrency caps, fallback.
- **v0.5 - join the room:** human-in-the-loop chat (HumanChannel + `--chat` TUI).
- **v0.6 - FlowMeter:** convergence/divergence measurement (observe-only).
- **v0.7 - MetaController:** macro->micro feedback (cool the room as it converges). The engine-layer roadmap - rhythm -> chemistry -> LLM -> concurrency -> chat -> flow-meter -> meta-controller - completes here.
- **v0.8 - friend engine (first increment):** participation-based memory + keyword recall + recall-eval harness.
- **v0.9 - friend engine, deeper:** Korean morphology + SQLite/FTS5 BM25 recall + cross-session persistence (feature-gated).
- **v0.10 - semantic recall:** BGE-M3 embeddings (ONNX) + HNSW (usearch) + hybrid BM25/vector RRF fusion (feature-gated).
- **web - topic debate app:** the engine becomes a product. A producer (debate modes, format variation, twist cards, a friendly register) + four-axis personality/voice personas + first-class human participation + closing reports + read-only sharing + an archive of past debates. See [Topic debate app](#topic-debate-app-web) below.

---

## Local LLMs (v0.3)

v0.3 wires in Ollama so personas generate actual lines. The engine still decides **who speaks and when** - deterministic as always. The LLM only fills in the content.

Default run stays LLM-off (FakeBackend) - byte-identical to v0.1 output, zero network needed. Pass `--llm` to opt in.

When real text is in play, two content-based RRF signals activate: **interest** (how much a new topic pulls a persona in) and **echo** (whether a persona is still processing what was just said). These sit on top of the existing λ/fairness/randomness signals.

A dedicated example, `persona_collapse`, puts the same model under two different persona prompts and logs both outputs side by side - watching whether a small model actually holds distinct personas over time or collapses toward a uniform voice.

## Concurrent / mixed-model (v0.4)

v0.4 adds a **backend pool** with two protocols:

- **Ollama** `/api/generate` - e.g. `gemma4:31b-cloud` (cloud, concurrency cap 3)
- **OpenAI-compatible** `/v1/chat/completions` - e.g. a friend's vLLM server (`qwen3.6-35b-fast`, concurrency cap 1)

**Per-persona routing** means a single room can mix models: some personas talk through one backend, some through another. A `mixed_bench` example puts both in the same room.

The live tick loop stays **sequential** (one speaker per tick, causal turn-taking). Concurrency is used for compare/bench via `generate_batch` - running the same prompt against multiple backends in parallel to compare persona tone or benchmark latency.

Defaults to cloud models - no local RAM/GPU load, the local daemon just proxies the request remotely. Local model loading is guarded against.

Per-backend semaphores enforce the concurrency cap. If a backend returns 4xx or times out, a fallback chain kicks in (next backend, or FakeBackend) - no panics.

**Real mixed-model output** (`cargo run --example mixed_bench`):

```
cloud  : gemma4:31b-cloud (cap=3)   friend : qwen3.6-35b-fast (cap=1)
routing: summarizer -> friend, others -> cloud
opening> 오늘 비 와서 다들 약속 취소했대. 좀 심심하네.

[friend via cloud]      비 오는 날엔 원래 좀 늘어지기 쉽지. 여기 커피나 마시면서 멍 때려.
[chaos via cloud]       그럼 우리 집 거실에서 비 구경 대회나 열까?
[summarizer via friend] 혼자 남아 있는 공간은 생각할 시간이 충분해진다.
```

The summarizer, routed to the larger friend model, reads quieter and more reflective - persona tone distinction holds even across different models.

---

## Join the room (v0.5)

v0.5 turns tunaSalon into the thing it was always building toward: a chat room you can actually join.

`cargo run -- --chat` opens a three-panel TUI: a scrolling **chat pane** on the left (all utterances, labelled by persona), a **gauge sidebar** on the right (each persona's live λ vs. the θ line), and a **text input box** at the bottom. You type; the personas react.

The mechanism is **HumanChannel**: when you send a message, it fires as a large external event in the Hawkes engine - strongly exciting every persona's urge and pulling the topic toward you (design §5). Personas that were mid-chatter turn toward you; when you go quiet, the room drifts back to its own rhythm.

LLM generation (~1.6s/line) runs off the main thread, so the UI stays responsive while a persona is "thinking." Replies come one at a time - causal turn-taking, same as always. Personas can be routed to different backends (e.g. a cloud model + a friend's vLLM `qwen3.6-35b-fast`), with fallback if one is down.

`--chat` requires a real terminal. In a non-interactive shell it prints a graceful error and exits.

**Live demo** (`cargo run --example chat_demo` - no terminal needed, runs non-interactively):

```
친구: Hey! What's up?
혼돈: I've decided to replace all your socks with slightly damp sponges. Ta-ta!
나: 안녕, 다들 비 와서 뭐해?          ← the human joins
친구: 난 그냥 집에서 뒹굴거리는 중! ㅋㅋㅋ   ← persona turns to react (switches to Korean)
```

When the human speaks, the room's attention shifts.

---

## FlowMeter - conversation thermometer (v0.6)

Is the room still alive, or is everyone just circling the same ground?

v0.6 adds a **convergence gauge**: a number between 0 and 1 that tracks whether recent utterances are pulling toward the same vocabulary (converging - repetitive, cooling) or scattering into fresh territory (diverging - lively, still warm).

Implementation is deliberately cheap: **token-overlap approximation** - average pairwise Jaccard similarity over the most recent utterances. No models required. BGE-M3 embeddings can drop in behind the same interface later when a more precise signal is needed.

This is **observe-only**: the gauge measures and displays; it does not feed back into the engine. No parameter changes, no automatic room adjustments. That feedback loop is v0.7 (MetaController) - deliberately last, because it's the least stable.

Where it shows up:
- TUI sidebar: a live convergence gauge next to the λ bars ("흐름 수렴 …").
- `cargo run --example chat_demo`: a per-line `[흐름] 수렴` readout after each utterance.

**Real `chat_demo` transcript** - convergence stays near zero because the chaos persona keeps throwing new topics:

```
혼돈: 사실 너의 발가락 사이에서 비밀 사회가 운영 중이라는 소문을 들었어...
  [흐름] 수렴 0.00
나: 얘들아 각자 자기소개 좀 해줄래?
  [흐름] 수렴 0.01
혼돈: 난 그냥 여기저기 불 지르고 다니는 혼돈의 전도사야!
  [흐름] 수렴 0.04
```

*(convergence ∈ [0,1]: 0 = all-novel / diverging / lively - 1 = repetitive / converging / cooling.)*

Determinism preserved: the metric is computed only from utterance content. In the default LLM-off run, utterances are fake and content-free, so the gauge is absent - headless golden output stays byte-identical.

---

## MetaController - closing the feedback loop (v0.7)

v0.6 measured convergence. v0.7 **acts on it**.

When the conversation **converges** - people circling the same ground - the engine lowers **μ (base chattiness)** via a cooling factor `mu_scale` (∈ [floor, 1]). The room speaks less, lets silence stretch, and winds down on its own. When talk is diverging and lively, the controller leaves μ alone.

This is a **heuristic** macro->micro layer, not an LLM. One direction only: convergence -> cooling.

**Stability is the main design concern** - a feedback loop can oscillate or get stuck, and this is the least stable part of the whole system. Two guards are in place:
- **Weak gain by default**: the cooling response is deliberately slow (tunable via `SALON_META_GAIN`).
- **Floor**: `mu_scale` never drops below ~0.4, so the room never goes permanently silent.

**Observe it**: the TUI sidebar and `chat_demo` show a "식힘 ×{value}" (cooling) line. `1.00` means no cooling is active.

**Content-gated / golden preserved**: with no LLM content (the default deterministic run), there is no convergence signal, so `mu_scale` stays at 1.0 and the engine is byte-identical to before. The deterministic headless path is unchanged.

A note worth stating: in the chaos-room demo, the conversation keeps diverging, so the cooling gauge idles at `식힘 ×1.00`. That is correct behavior - cooling only engages when a conversation actually circles a topic long enough to cross the convergence threshold. A room that never converges never gets cooled.

---

## Long-term memory - friend engine, first increment (v0.8)

The original engine-layer roadmap (v0.1-v0.7) is complete. v0.8 opens a separate track: **personas that remember**. A persona who's been in the room with you starts recalling what was said and weaves it back into what it says next.

This first increment is deliberately small:
- **Participation-based memory**: an in-memory store of events `{room, ts, speaker, content}` plus who was present in each room. A persona can only recall from rooms it actually sat in - no recalling a conversation it wasn't part of.
- **Keyword recall**: token-overlap scoring (reusing the FlowMeter tokenizer) picks the top-K past lines for the current context. Injected into the recall slot the v0.3 context interface already reserved.
- **Recall eval harness**: the real payoff. A headless scenario plants a known fact (SSOT) plus distractors across rooms, then auto-scores the retrieval layer for recall/precision and participation isolation - deterministically.

What's **not** here yet (later): BGE-M3 semantic search, SQLite persistence (L1), forgetting, subjective per-persona storage, cross-room impressions of people.

**Content-gated / golden preserved**: recall is wired into the **live chat path only**. The deterministic headless/driver path never injects recall, so with no LLM content there are no events, no recall, and the output stays byte-identical to v0.1.

---

## Deeper recall + persistence (v0.9)

v0.8 remembered in-memory with raw token overlap. v0.9 deepens it by lifting the search core from the author's own [seCall](https://github.com/hang-in/seCall) engine - all behind a `friend-engine` Cargo feature (off by default, so the normal build pulls no extra deps and the golden output is untouched):

- **Korean morphology** (Stage 0): recall tokenizes with Lindera (ko-dic), stripping 조사/어미 - so "비가 온다" now matches a past "비 온다" line that whitespace overlap missed.
- **SQLite + FTS5 BM25** (Stage 1): the store becomes real SQLite with an FTS5 index; recall ranks past lines by BM25 (term frequency / rarity / length) with OR-match, instead of a raw token count.
- **Cross-session memory**: `--chat` persists to `~/.local/share/tunaSalon/memory.db` (`$SALON_MEMORY_DB` overrides), so personas remember across runs. Tests stay in `:memory:` and write nothing to disk.

Alongside v0.9 the `--chat` room got livelier - a tuned 3-way config (cross-excitation + no same-speaker-twice) and `/topic` to steer the conversation (gated by `smoke_chat`).

---

## Semantic recall (v0.10)

v0.9 recall was lexical (BM25 over morphemes) - it misses lines that share meaning but no words. v0.10 adds **semantic recall** on top, behind a `friend-engine-semantic` sub-feature (v0.9 still builds without the ML stack; the default build stays ML-free and golden-clean):

- **BGE-M3 embeddings** (Stage 2a): utterances are embedded in-process via ONNX Runtime (`ort`, download-binaries, optional CoreML) - no daemon, no Ollama. Measured on this Mac: ~3.8 s load, ~29 ms/embed, ~2.3 GB. A deterministic `MockEmbedder` keeps everyday tests reproducible.
- **HNSW ANN** (Stage 2b): vectors go into a usearch index (cosine); each embedding persists as a SQLite BLOB next to the memory.
- **Hybrid RRF** (Stage 2c): recall fuses the BM25 (lexical) and vector (semantic) legs with Reciprocal Rank Fusion (k=60), both participation-isolated.
- **Live wiring** (Stage 2d): `--chat` loads the real BGE-M3 embedder when the model is present (Mock fallback otherwise). The embedder is kept consistent per-DB (`meta.embedder_kind`); switching it triggers a full re-embed so Mock and Ort vectors never mix.

So lexically-different-but-semantically-same lines now recall: a stored "강아지랑 산책 다녀왔어" surfaces for the query "반려동물 데리고 나갔어" (verified by an `#[ignore]` real-model test).

---

## Topic debate app (web)

The engine track (v0.1-v0.10) was always means to an end: liveliness. That liveliness is now a product you open in a browser (axum WebSocket + React, behind `--features "web redis-bus"`). The engine wasn't touched: it just gained one more sink.

A debate of competent essays is boring. The fun comes from a **producer** layer, and it lives in its own framework-independent module `src/debate/` so the engine files don't bloat:

- **DebatePlan**: a topic maps, deterministically (no LLM meta-call), to a debate mode - Courtroom, PolicyDuel, MoralDilemma, PersonalStakes, Forecasting, DesignReview, plus a CasualBanter mode for light topics (mint-choc, pineapple-on-pizza). "AI 판사가 공정할까?" becomes a courtroom; a mint-choc fight becomes casual banter.
- **Format variation + twist cards**: a per-turn hint cycles through formats (cross-examine, steelman, one concrete case, a threshold, a conditional concession). When the room converges (FlowMeter) or a speaker repeats, a fresh development is injected to force a reaction.
- **Friends, not a courtroom**: even serious topics keep a casual register ("argue like friends over dinner, not lawyers"). The analytical focus (evidence, premises, trade-offs) stays; only the register softens.

All of it is injected only into the generation snapshot, never the stored history, so the default build and headless golden output stay byte-identical.

**Personas get personality.** They're assembled from blood-type / MBTI / zodiac (four axes), which set not just *who* a persona is (a trait) but *how* they talk (a voice directive). An ENTP cuts in with "wait, but-" and a flipped question; a blood-type-A hedges; a Pisces answers in metaphor. The same model splits into distinct voices.

**You're a first-class participant.** Jump in and every persona's urge spikes (v0.5's HumanChannel, unchanged); for a few turns your message is the top priority; call a nickname and that persona answers. Toss a line into a finished debate and it reopens.

**When a debate ends, a closing report.** A neutral analyst writes a conclusion-first Markdown report: the verdict, each participant's stance (yours included), points of agreement, and what stayed unresolved.

**A good debate can be shared.** An opaque token link lets anyone read the full transcript (avatars, colours, conclusion) with no login, on desktop or mobile - only rooms you explicitly share. Every past room is kept in an **archive** with its start and conclusion dates; pick one and re-enter to restore participants, topics, and history (`rooms.db` persistence).

**Redis multi-session** (`redis-bus` feature, opt-in via `SALON_REDIS_URL`) is a volatile coordination layer (room command/event streams, owner lease, presence). Not a memory store: `LiveSession` stays the single writer per room and SQLite (`memory.db`, `rooms.db`) stays the durable source of truth. Flushing Redis never loses history.
