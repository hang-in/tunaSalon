---
title: Salon web 채팅 UI 디자인 프롬프트 (Kimi 2.6 agent 위임용)
type: reference
status: active
updated_at: 2026-06-03
ref: ../plans/salon-web-frontend.md
---

# Salon web 채팅 UI 디자인 프롬프트

web 프런트엔드 트랙([salon-web-frontend.md](../plans/salon-web-frontend.md))의 UI 디자인을 외부 코딩 에이전트(Kimi 2.6)에 위임하기 위한 프롬프트. 우리 엔진의 실제 데이터 모델(λ·θ·flow 수렴·mu_scale 식힘·topics·recall)과 "생동감"(페르소나가 안달하며 턴을 잡는 리듬)을 핵심으로 박았다.

**산출물 기대**: 정적 HTML/CSS/JS(또는 Svelte) + mock WebSocket 레이어 → 브라우저에서 바로 살아 움직이는 디자인. 이후 우리 axum WebSocket 데이터 계약(아래 frames)에 맞춰 `connect(onFrame)` 심만 교체.

---

## 프롬프트 (그대로 복붙)

```
You are a senior frontend/UX engineer. Design and build a polished, production-grade WEB chat UI for an app called **tunaSalon**. Deliver a self-contained, runnable front-end (HTML + CSS + vanilla JS, or Svelte if you prefer — no heavy SPA framework) with a MOCK data layer so it can be opened in a browser and demoed live without any backend.

## What tunaSalon is
A chat room where a single human ("나") makes small talk with 2–4 LLM personas. The twist — and the thing the UI must make beautiful — is the **conversation-flow engine** behind it. Personas don't reply round-robin; each has a rising "urge to speak" (an intensity λ). When λ crosses a threshold θ they may speak; speaking drains their λ; if everyone is below θ the room goes quiet; one persona speaking excites the others. The signature experience is **watching the personas get antsy, take turns, fall silent, and rile each other up** — a living room, not a request/response bot.

The Rust engine is already built and will stream state to the browser over a WebSocket. **You only build the front-end.** Mock the WebSocket with a JS module that emits realistic frames on timers so the design is visibly alive in a static demo; isolate it behind one function (e.g. `connect(onFrame)`) so a real `ws://` endpoint can be swapped in later.

## The data contract (design to these exact frames)
Server → client (JSON, one object per message):
- `{ "type":"state", "intensities": {"friend":0.72,"realist":0.55,"summarizer":0.28}, "theta":0.60, "flow":0.08, "mu_scale":1.0, "pending":"friend", "participants":[{"id":"friend","name":"Friendly Regular"},{"id":"realist","name":"Grounded Realist"},{"id":"summarizer","name":"Quiet Summarizer"},{"id":"나","name":"나"}], "topics":["부처님 오신날"] }` — sent ~every 0.5–2s. `intensities` = each persona's λ (0..~1). `theta` = the speak threshold (same for all). `pending` = id of a persona currently generating, or null. `flow` = conversation convergence 0..1 (0 = diverging/lively, 1 = converging/circling). `mu_scale` = room "cooling" factor 0.4..1.0 (1.0 = not cooling). The human "나" has NO λ (it's an external participant).
- `{ "type":"utterance", "speaker":"friend", "name":"Friendly Regular", "content":"부처님 말씀 중에 집착을 버리라는 게 핵심인 것 같아.", "ts": 173 }` — a new chat message lands.
- `{ "type":"recall", "speaker":"friend", "note":"지난 대화에서: 너 등산 좋아한다고 했지" }` — optional: a persona is drawing on long-term memory of past sessions (show subtly).
- `{ "type":"system", "text":"화제가 '국제정세'로 바뀌었습니다" }` — room notices.

Client → server:
- `{ "type":"message", "text":"어린이날엔 뭐하지?" }` — the human sends a message. (A human message makes every persona's λ spike — render that "all heads turn to 나" moment.)
- `{ "type":"topic", "topics":["국제정세","주말 계획"] }` — set 1–5 room topic tags.

## Make the "aliveness" the centerpiece (most important)
This is NOT a generic chat app. The per-persona **λ gauges are first-class, not a side widget.** Design them so the user FEELS the rhythm:
- Each persona shows a live "speaking urge" meter rising toward a clearly marked **θ line**. As λ nears θ, the persona looks antsy (subtle motion/glow/lean-in). Crossing θ + being chosen → it speaks → its meter visibly drops (relief). Smooth CSS transitions, not jumpy.
- **Silence**: when all λ are below θ, the room is visibly quiet (gauges slowly climbing, a calm hush) — anticipation, not emptiness.
- **Human interrupt**: when 나 sends a message, all λ spike at once (everyone turns to the human). Make this a satisfying beat.
- **Convergence thermometer**: a tasteful indicator of `flow` (room circling the same ground vs. lively/diverging) and `mu_scale` (the room "cooling down" — speaking less as it converges).
- Optional flourish: a per-persona avatar/face whose expression or posture reflects its λ band (idle → fidgety → "hand up, about to speak" → speaking → satisfied).

## Layout & components (adapt as you see fit)
- Header: app name, room/topic chips (the 1–5 `topics`), connection/participant status.
- Main: a chat transcript with auto-scroll, per-persona color/avatar, readable typography, message grouping, and rendering of multi-sentence Korean text + light markdown. A subtle "thinking…" indicator for the `pending` persona.
- Side panel (a true presence/meter panel, not just a roster): each participant with their live λ gauge + θ marker; the human shown as a participant without a λ; the convergence/cooling indicators.
- Composer: a real text input (cursor, multiline, IME-friendly for Korean), send on Enter, plus a way to set topics (a "/topic" affordance or a topics editor).
- Empty/loading/error states; graceful on window resize; works at narrow widths.

## Production-grade bar
Smooth 60fps gauge animations, coherent dark theme (light optional), accessible (contrast, keyboard, reduced-motion fallback), no layout jank, no external CDNs required to run, fast. It should feel like a real product someone shipped — the kind of UI that makes a visitor think "oh, this is properly made." Korean UI copy (the app is Korean-facing); content samples above are Korean on purpose.

## Deliverable
1. A runnable static front-end (open index.html → see it live via the mock WS, with personas getting antsy, taking turns, the human able to type and set topics).
2. The mock data module clearly isolated behind a single `connect(onFrame)` / `send(frame)` seam, with a one-line note on swapping in a real `ws://localhost:PORT/ws`.
3. A short README: design rationale (esp. how you visualized the λ→θ rhythm), the component breakdown, and what you'd refine next.

Personas for the demo: Friendly Regular (warm), Grounded Realist (matter-of-fact, grounds exaggeration), Quiet Summarizer (speaks rarely, ties threads). Aim for a calm, modern, slightly cozy "chat lounge" aesthetic. Surprise me with the aliveness visualization — that's the soul of the app.
```

---

## 검토 시 확인 포인트(Kimi 산출물 받으면)

- mock WS frames가 위 계약과 일치하는가 → 우리 axum WebSocket 어댑터가 그대로 emit 가능한가.
- λ→θ 리듬 시각화가 실제로 "생동감" 있는가(엔진의 차별점).
- 정적·CDN 불요·자체 완결인가(서버 없이 demo).
- 골든/엔진과 무관(프런트 전용). 키는 서버에만(브라우저 노출 0).
