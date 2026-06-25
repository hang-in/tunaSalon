# Claude Code Handoff: tunaSalon Debate App

Updated: 2026-06-26
Repo: `D:\privateProject\tunaSalon`

## Conclusion

tunaSalon is being upgraded from a casual multi-agent chat demo into a topic-based LLM debate app.

The current system already has the important infrastructure pieces:

- Redis is for volatile multi-session coordination.
- SQLite `rooms.db` is for durable room checkpoints.
- SQLite `memory.db` is for long-term persona memory and recall.
- Web dev must run on port `6173`; backend API/WebSocket currently runs on `8080`.
- The debate quality still needs a proper "debate producer" layer: room creation meta planning, topic-specific debate modes, hidden persona goals, format variation, and anti-loop moderation.

Do not turn Redis into the memory database. Keep `LiveSession` as the single-writer room engine.

## Current Runtime Snapshot

This was the state after the latest Codex pass:

- Backend: `target\debug\salon.exe`, `127.0.0.1:8080`
- Frontend dev: Vite on `0.0.0.0:6173`
- LAN URL: `http://192.168.1.179:6173/`
- Friend backend healthcheck was failing, so routing fell back to cloud-only.
- Frontend port `5173` must stay free for another app.

These PIDs are only a snapshot and may be stale:

- Backend was PID `31932`
- Frontend was PID `26824`

Check before assuming:

```powershell
Get-NetTCPConnection -LocalPort 8080,6173 -State Listen -ErrorAction SilentlyContinue |
  Select-Object LocalAddress,LocalPort,OwningProcess
Get-Process salon -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,Path
```

## User's Product Direction

The user wants a "topic debate room" product:

- Lobby shows topic rooms.
- User enters a room, then the debate starts or resumes.
- Leaving a room should pause/stop LLM generation.
- Re-entering should continue the same discussion.
- Rooms can be reset and deleted.
- The user can create multiple rooms.
- User messages mid-debate should be treated as interventions that participants answer.
- Debate should feel like participants remember, rebut, agree, call each other by nickname, and maintain persona-specific perspectives.

The user specifically noticed these problems:

- Some generated content was unrelated or generic.
- Personas sometimes mentioned a non-existent `chaos` persona.
- Models used awkward direct address like `나님`.
- Discussion was repetitive and too formal.
- Existing persona prompts were overfit to "AI regulation and open source", so new topics such as "AI judge fairness" still drifted into open-source governance.
- Pause UI showed `일시정지됨`, but in-flight LLM responses still appeared afterward.

## Implemented So Far

### Redis / Multi-Session

Relevant files:

- `src/session_bus.rs`
- `src/web.rs`
- `src/main.rs`
- `Cargo.toml`

Implemented shape:

- `redis-bus` feature exists.
- Room command/event bus is abstracted enough that `LiveSession` remains Redis-free.
- Room owner lease exists via Redis.
- Non-owner gateway path can forward commands/events.
- Command cursor persistence exists with `room:{room_id}:cmd:cursor`.
- Room id boundary exists throughout web startup.

Redis is volatile coordination only:

- commands
- events
- owner lease
- presence
- short hot state

DB remains authoritative for durable history/memory.

### Room Persistence / UX

Relevant files:

- `src/roomstore.rs`
- `src/memory.rs`
- `src/web.rs`
- `web/src/App.tsx`
- `web/src/hooks/useChat.ts`
- `web/src/lib/realEngine.ts`

Implemented:

- Room checkpoints by `room_id`.
- Room restore with participants, topics, messages, tick count.
- Lobby with user-created/recent rooms.
- Default recommended room cards were removed.
- Duplicate lobby cards are deduped.
- Room delete API exists.
- Room reset exists.
- Topic placeholders rotate from ten fun topic suggestions.
- Empty topic create uses the current placeholder.
- Vite host is `0.0.0.0`, port `6173`, strict port.

### Debate Quality Fixes

Relevant files:

- `src/live.rs`
- `src/main.rs`
- `src/openai.rs`
- `src/ollama.rs`

Implemented:

- Reasoning/thinking is enabled for current debate-oriented backend path.
- Persona prompts now demand 3-6 substantial sentences.
- `나님` / `나 님` are sanitized to `사용자님`.
- User mentioning a persona name/id forces that persona as next speaker.
- A summarizer persona is periodically forced after several turns.
- Repetition guard injects a "new evidence / metric / threshold / mechanism / compromise" instruction.
- Recall query uses more recent context lines.
- Cross-room memory is filtered so a broad token like `AI` does not pull in old room topics.
- Default base personas were made topic-neutral instead of open-source-specific.
- Pause now cancels pending generation by bumping generation epoch and removing empty placeholders.

Important pause nuance:

- Already-sent HTTP requests are not forcibly aborted.
- Their late results are ignored and will not be emitted, stored, or remembered.

## Recent Validation

The latest pass ran:

```powershell
cargo fmt
pnpm -C web lint
cargo test --features "web redis-bus" live::tests::cancel_pending_generation_removes_placeholder_and_ignores_late_result
cargo test --features "web redis-bus" live::tests::recall_for_generation_filters_weak_cross_room_topic_overlap
cargo test --features "web redis-bus" web::tests::effective_paused_tracks_manual_presence_and_backend
cargo test --features "web redis-bus" openai::tests::assemble_user_prompt
cargo test --features "web redis-bus" ollama::tests::assemble_user_prompt
cargo build --features "web redis-bus"
pnpm -C web build
git diff --check
```

Notes:

- `git diff --check` only emitted LF/CRLF warnings.
- `pnpm -C web build` emitted the existing large chunk warning.
- During Rust builds, stop running `target\debug\salon.exe`; otherwise Windows may block replacing `target\debug\salon.exe`.

## How To Restart Dev

Backend:

```powershell
$p = Get-NetTCPConnection -LocalPort 8080 -State Listen -ErrorAction SilentlyContinue |
  Select-Object -First 1 -ExpandProperty OwningProcess
if ($p) { Stop-Process -Id $p -Force }

$logDir = Join-Path (Get-Location) 'target\run-logs'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
$out = Join-Path $logDir 'salon-web.out.log'
$err = Join-Path $logDir 'salon-web.err.log'
Remove-Item $out,$err -ErrorAction SilentlyContinue

$args = '--web --host 127.0.0.1 --port 8080 --room-id debate-ai-open-source --topic "AI 규제와 오픈소스"'
Start-Process -FilePath (Resolve-Path 'target\debug\salon.exe') `
  -ArgumentList $args `
  -WorkingDirectory (Get-Location) `
  -WindowStyle Hidden `
  -RedirectStandardOutput $out `
  -RedirectStandardError $err `
  -PassThru
```

Frontend:

```powershell
$p = Get-NetTCPConnection -LocalPort 6173 -State Listen -ErrorAction SilentlyContinue |
  Select-Object -First 1 -ExpandProperty OwningProcess
if ($p) { Stop-Process -Id $p -Force }

Start-Process -FilePath "pnpm.cmd" `
  -ArgumentList @('-C','web','dev','--host','0.0.0.0','--port','6173') `
  -WorkingDirectory (Get-Location) `
  -WindowStyle Hidden `
  -PassThru
```

Verify:

```powershell
Invoke-WebRequest -UseBasicParsing http://127.0.0.1:8080/ | Select-Object StatusCode,StatusDescription
Invoke-WebRequest -UseBasicParsing http://192.168.1.179:6173/ | Select-Object StatusCode,StatusDescription
```

## Why The Debate Still Feels Boring

Current system mostly asks LLMs to "give a good debate response." That produces competent essays, but not a fun debate.

The missing product layer is a debate producer:

- It decides what kind of debate the topic should become.
- It assigns participants asymmetric roles and private objectives.
- It varies speaking formats.
- It injects new evidence, constraints, reversals, or audience pressure when the discussion loops.
- It decides when summarizer should summarize, challenge, or reframe.
- It tracks unresolved claims and forces participants to answer them.

Without this, models repeat a polished version of the same argument and the debate becomes a policy memo exchange.

## Debate Fun Devices To Implement

### 1. Room-Creation Meta Agent

When a room is created, infer a `DebatePlan` from the topic.

Suggested schema:

```rust
pub struct DebatePlan {
    pub topic: String,
    pub mode: DebateMode,
    pub opening_question: String,
    pub stakes: String,
    pub fault_lines: Vec<String>,
    pub evidence_cards: Vec<EvidenceCard>,
    pub persona_assignments: BTreeMap<PersonaId, PersonaDebateRole>,
    pub format_cycle: Vec<UtteranceFormat>,
}
```

This can be deterministic at first. Do not require an LLM meta call for v1.

Example modes:

- `PolicyDuel`: laws, regulation, institutions, enforcement.
- `MoralDilemma`: tradeoffs, harm, rights, dignity.
- `Courtroom`: claims must cite evidence, opposing counsel cross-examines.
- `Forecasting`: participants make predictions and confidence estimates.
- `DesignReview`: participants argue over product/system design.
- `PersonalStakes`: topic is grounded in everyday life, not abstract policy.
- `DevilsTriangle`: three agents each defend a different bad-but-plausible tradeoff.

Mapping examples:

- "AI 판사가 인간 판사보다 공정할 수 있을까?"
  - Mode: `Courtroom` or `MoralDilemma`
  - Fault lines: bias vs consistency, appeal rights, explainability, accountability
  - Evidence cards: COMPAS, EU AI Act high-risk systems, human judge sentencing disparity studies

- "AI 규제와 오픈소스"
  - Mode: `PolicyDuel`
  - Fault lines: safety vs innovation, voluntary maintainers vs commercial users, liability vs commons
  - Evidence cards: EU CRA, Log4Shell, xz-utils, model weight release policy

Where to inject:

- Add fields to `WebStartup` or room metadata.
- Store in `rooms.db` so restored rooms keep their debate plan.
- Inject concise plan text into generation directive, not into visible UI.

### 2. Hidden Persona Goals

Each persona needs a public stance and a hidden objective. The hidden objective should shape behavior without being revealed.

Suggested schema:

```rust
pub struct PersonaDebateRole {
    pub public_stance: String,
    pub private_goal: String,
    pub fear: String,
    pub win_condition: String,
    pub concession_trigger: String,
    pub taboo: String,
    pub pressure_style: String,
}
```

Examples for "AI judge fairness":

- Civic-benefit advocate:
  - Public stance: AI can help fairness only as assistive and transparent.
  - Private goal: force the debate to protect appeal rights and vulnerable defendants.
  - Fear: efficiency will be used to hide institutional bias.
  - Win condition: opponent accepts mandatory human review and public audit logs.
  - Concession trigger: accepts AI triage for low-stakes administrative disputes.
  - Taboo: never claim AI should fully replace judges.

- Implementation realist:
  - Public stance: AI judge is only acceptable under measurable error and liability controls.
  - Private goal: expose every vague transparency claim as unauditable.
  - Fear: "fairness" becomes a slogan with no operational owner.
  - Win condition: opponent accepts concrete thresholds, audit liability, and rollback rules.
  - Concession trigger: accepts pilot programs with narrow domain and appeal override.
  - Taboo: never rely on "trust the experts."

- Summarizer/challenger:
  - Public stance: neither side has solved accountability.
  - Private goal: identify hidden premise shifts and force a sharper decision point.
  - Fear: debate collapses into examples without a policy test.
  - Win condition: room reaches a named unresolved question or compromise experiment.
  - Concession trigger: when both sides name a measurable standard.
  - Taboo: do not merely summarize; challenge one missing premise.

Rules:

- Do not show hidden goals in UI.
- Do not let models reveal them.
- Inject them as private system/debate-plan text.
- Hidden goals should not override the topic; they are pressure vectors.

### 3. Utterance Format Variation

Current length hints produce essay-like responses. Add format hints so turns feel different.

Suggested enum:

```rust
pub enum UtteranceFormat {
    OpeningPosition,
    DirectRebuttal,
    CrossExamination,
    SteelmanThenAttack,
    ConcreteCase,
    QuantifiedThreshold,
    ConditionalConcession,
    DilemmaFork,
    Prediction,
    SummarizeAndForceChoice,
}
```

Prompt snippets:

- `CrossExamination`: ask exactly one pointed question, then explain why it matters.
- `SteelmanThenAttack`: restate opponent's strongest point fairly, then attack one premise.
- `ConcreteCase`: use one concrete case; do not add a second case.
- `QuantifiedThreshold`: propose one measurable threshold or rollback condition.
- `ConditionalConcession`: say what would change your mind.
- `DilemmaFork`: force a two-option tradeoff and ask the opponent to choose.
- `SummarizeAndForceChoice`: summarize disagreement in one sentence, then demand a decision test.

Implementation path:

- Replace `length_hint(tick, speaker)` in `src/live.rs` with `format_hint(tick, speaker, plan, speaker_role)`.
- Keep deterministic selection at first.
- Summarizer should get different format weights than adversarial speakers.

### 4. Argument Ledger

Track claims, unanswered questions, concessions, and repeated claims.

Suggested lightweight model:

```rust
pub struct DebateLedger {
    pub open_questions: Vec<String>,
    pub claims: Vec<Claim>,
    pub concessions: Vec<String>,
    pub repeated_topics: Vec<String>,
}
```

Do not build full NLP first. Start with LLM-facing text directives:

- "Last unresolved question: ..."
- "You have not answered: ..."
- "Do not repeat claim X unless adding new evidence."

Later, a meta agent can summarize the ledger every 4-6 turns.

### 5. Evidence Cards / Twist Cards

The debate becomes more entertaining when new constraints arrive.

Examples:

- "A wrongful conviction happened because the AI explanation was wrong."
- "A human judge in the same court has a documented sentencing disparity."
- "The AI vendor refuses to disclose training data."
- "A public defender says appeal workload doubled."
- "The system performs well overall but fails for one demographic subgroup."

Inject one card when:

- the same two personas repeat the same stance twice;
- no user message has arrived for N turns;
- summarizer detects no new claim;
- conversation is over-converging by `flow()`.

This should be hidden producer text, not a visible system message at first.

### 6. Persona Relationships

Participants should remember each other's debate behavior:

- "평화로운매의숨결 tends to demand enforcement thresholds."
- "지혜로운바람처럼 tends to protect participation and autonomy."
- "날카로운별의노래 tends to reframe hidden premises."

This makes nickname references feel earned.

Implementation:

- Store relationship notes in memory or room plan.
- Inject one relevant relationship note per generation.
- Do not overdo it; one short note is enough.

### 7. User Interventions

When the user speaks, treat it as a host/intervention, not just another chat line.

Already partly implemented:

- `submit_human` sets `human_focus`.
- Mentioning a persona forces that persona next.

Improve:

- If user asks "왜 조용해?", force the named persona and require direct answer.
- If user changes topic, update room topic or create a subtopic marker.
- If user says "재밌게", producer should inject a twist card or format shift.
- If user says "정리해", force summarizer.

### 8. Better Topic Defaults

There are ten current placeholder topics in `web/src/App.tsx`.

Current set:

- `AI 친구는 진짜 친구가 될 수 있을까?`
- `죽은 사람의 말투를 복원한 AI는 위로일까, 모독일까?`
- `기본소득은 인간을 게으르게 만들까, 자유롭게 만들까?`
- `기억을 선택적으로 지울 수 있다면 지워도 될까?`
- `AI 판사가 인간 판사보다 공정할 수 있을까?`
- `연애 앱은 사랑을 돕는가, 소비하게 만드는가?`
- `아이에게 스마트폰을 주는 나이는 법으로 정해야 할까?`
- `인터넷 익명성은 보호해야 할 권리인가, 폐지해야 할 위험인가?`
- `완전 자동화 사회에서 일하지 않는 사람도 존중받을 수 있을까?`
- `가족보다 선택한 공동체가 더 중요해질 수 있을까?`

These are better than generic policy topics because they contain moral tension, personal stakes, and concrete tradeoffs.

## Recommended Next Implementation Order

### Step 1: Add DebatePlan Types

Add a small module, likely `src/debate_plan.rs`.

Keep it deterministic and testable:

- `infer_debate_plan(topics: &[String]) -> DebatePlan`
- keyword/category matching first
- no LLM meta call yet

Expose:

- `mode`
- `opening_question`
- `fault_lines`
- `evidence_cards`
- per-persona debate role templates
- format cycle

Tests:

- AI judge topic maps to courtroom/moral dilemma.
- open-source regulation maps to policy duel.
- romance app topic maps to personal stakes / moral dilemma.

### Step 2: Persist DebatePlan Per Room

Extend `RoomStore` metadata:

- Add `debate_plan_json` column or new table.
- On missing column, migrate safely.
- Existing rooms can infer plan from topics on load.

Avoid breaking older DBs.

### Step 3: Inject Plan Into Generation

Change `LiveSession` to hold `debate_plan: Option<DebatePlan>`.

Inject into directive:

- current mode
- current stakes
- speaker public stance
- private goal
- one format hint
- one open question or evidence card if needed

Keep it concise. Long prompts make models drift.

### Step 4: Replace Length Hint With Format Hint

`length_hint` currently makes the debate longer but still essay-like.

Add:

- `format_hint(tick, speaker, plan)`
- deterministic format cycle
- summarizer-specific formats

### Step 5: Add Loop-Breaking Producer

Use existing signals:

- `flow()`
- `repetition_guard`
- `turns_since_summary`
- repeated speaker arguments

Inject:

- evidence card
- force question
- forced summarizer challenge
- forced concession condition

### Step 6: Frontend Small UX Follow-Ups

Potential improvements:

- Show debate mode and short room summary on lobby cards.
- Do not show private persona goals.
- Show room state: live / paused / generating / waiting.
- Add "새 국면 투입" button later for manual twist card.

## Important Code Touchpoints

Backend:

- `src/live.rs`
  - core session loop
  - pending generation
  - human focus
  - speaker forcing
  - recall filtering
  - directive building

- `src/web.rs`
  - room runtime
  - pause/presence handling
  - reset/delete
  - WebSocket frames
  - Redis owner/gateway path

- `src/main.rs`
  - demo persona prompts
  - default persona profile refresh on room restore
  - cloud/friend routing
  - CLI startup

- `src/roomstore.rs`
  - durable room metadata
  - best place to persist debate plan

- `src/memory.rs`
  - long-term memory
  - do not mix with Redis

Frontend:

- `web/src/App.tsx`
  - lobby
  - room creation
  - placeholder topics
  - room cards

- `web/src/hooks/useChat.ts`
  - WebSocket state/messages

- `web/src/lib/realEngine.ts`
  - real WebSocket connection

- `web/src/components/Header.tsx`
  - pause/reset/delete/leave controls

## Risk Notes

- Existing dirty worktree is large. Do not revert unrelated changes.
- `cargo fmt` may touch many files because of earlier formatting drift.
- Running backend locks `target\debug\salon.exe` on Windows. Stop it before Rust builds.
- Friend backend is currently unreliable/unavailable. Keep cloud-only fallback.
- Existing rooms may contain old bad history. Reset or create a new room when judging debate quality.
- Stored rooms can contain old persona prompts, but default personas are refreshed in `main.rs` during restore via `apply_default_persona_profile`.

## Acceptance Criteria For Next Claude Code Pass

The next pass is successful if:

- A new topic room gets a deterministic `DebatePlan`.
- Different topics produce visibly different debate modes.
- Personas have hidden goals that affect stance without being exposed.
- Turns vary by format, not only by length.
- A repeated debate receives a producer intervention.
- "AI 판사가 인간 판사보다 공정할 수 있을까?" no longer defaults to open-source arguments unless the user explicitly brings them in.
- Pause still prevents late in-flight LLM responses from appearing.
- `cargo test --features "web redis-bus" live::tests` passes.
- `cargo build --features "web redis-bus"` passes.
- `pnpm -C web lint` and `pnpm -C web build` pass.

