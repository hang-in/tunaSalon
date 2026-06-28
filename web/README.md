# tunaSalon

A production-grade chat UI for tunaSalon - a "living room" where a human chats with 2-4 LLM personas driven by a conversation-flow engine. Each persona has a rising "urge to speak" (λ intensity) that the UI makes viscerally alive.

**Live demo**: https://h37ajp3cbfsky.kimi.page

---

## What Makes This Different

This is **not** a request/response chatbot UI. The signature experience is **watching the personas get antsy, take turns, fall silent, and rile each other up**. The per-persona λ gauges are first-class visuals - not a side widget - designed so the user *feels* the rhythm of the conversation.

### Visualizing the λ→θ Rhythm

The core design decision was to treat each persona's speaking-urge gauge as a **living, breathing thing**:

- **Gauge anatomy**: Each gauge is a horizontal bar with a clearly marked θ threshold line. The fill color is unique per persona (coral for friend, sage for realist, lavender for summarizer). As λ rises, the bar smoothly fills with a `cubic-bezier(0.25, 1, 0.5, 1)` transition - organic, not mechanical.
- **Anticipation states**: As λ crosses 70% of θ, a **subtle glow** (`box-shadow` in the persona's color) appears on the gauge. The persona card border tints. The avatar expression shifts from "idle" to "fidgety" - communicated through a simple but effective emoji-like glyph system (◡ → ‿ → •̀ → ☝ → ◠).
- **The speak moment**: When λ crosses θ and the persona is chosen, the gauge flashes white with a **shimmer gradient animation** (`gauge-shimmer` keyframes), then visibly drains as the utterance is delivered - a satisfying "relief" visual.
- **Silence mode**: When all λ are below θ, a **CSS vignette overlay** breathes gently over the chat area, creating anticipation rather than emptiness. The sidebar shows a "조용한 순간" (quiet moment) badge.
- **Human interrupt**: When "나" sends a message, all three AI personas' λ gauges **instantly spike** (via the mock engine's `onHumanMessage`), creating a satisfying "all heads turn" beat.
- **Three.js living room**: A subtle 3D background (rotating metallic cube + per-persona point lights driven by real-time λ values) provides atmospheric depth, fading out as the conversation gets going.

---

## Architecture

### Tech Stack
- **React 18 + TypeScript + Vite** - Core framework
- **Tailwind CSS** - Utility styling
- **Three.js** - 3D background scene
- **Lucide React** - Iconography
- **No external CDNs** - Everything is bundled

### File Structure

```
src/
  lib/
    mockEngine.ts        # Isolated mock WebSocket (swappable for real ws://)
  types/
    index.ts             # Shared TypeScript types
  hooks/
    useChat.ts           # Main state management hook
  components/
    Header.tsx           # Top bar: logo, topic chips, connection status
    ChatArea.tsx         # Chat transcript with auto-scroll, message grouping
    Composer.tsx         # Text input + topic editor
    SidePanel.tsx        # λ gauges, persona presence, global metrics
    LambdaGauge.tsx      # Individual λ gauge bar
    PersonaPresence.tsx  # Persona card with gauge + avatar expression
    ThreeBackground.tsx  # Three.js atmospheric background
  App.tsx                # Main layout
  index.css              # Global styles, animations, keyframes
```

### Mock Engine (`src/lib/mockEngine.ts`)

The mock is **isolated behind a single function**:

```typescript
import { connect } from "@/lib/mockEngine";

const conn = connect((frame) => {
  // handle incoming frames
});

conn.send({ type: "message", text: "hello" });
conn.disconnect();
```

**To swap in a real WebSocket**, replace this module with:

```typescript
export function connect(onFrame) {
  const ws = new WebSocket("ws://localhost:PORT/ws");
  ws.onmessage = (e) => onFrame(JSON.parse(e.data));
  return {
    send: (frame) => ws.send(JSON.stringify(frame)),
    disconnect: () => ws.close(),
  };
}
```

The mock engine simulates realistic conversation dynamics:
- Each persona's λ rises at a different rate (friend = chatty, summarizer = slow)
- When λ crosses θ, a "generation" delay occurs, then an utterance is emitted
- Speaking drains λ and slightly excites other personas
- Topic changes happen occasionally with system notices
- Human messages spike all λ simultaneously
- Recall frames emit sporadically (15% chance) for memory visualization

### Data Contract

**Server → Client frames:**
- `type: "state"` - intensities, theta, flow, mu_scale, pending, participants, topics
- `type: "utterance"` - speaker, name, content, ts
- `type: "recall"` - speaker, note (long-term memory indicator)
- `type: "system"` - text (room notices)

**Client → Server frames:**
- `type: "message"` - text (human message)
- `type: "topic"` - topics[] (set room topics)

---

## Design Decisions

### Color System
- Deep warm dark backgrounds (`#151515`, `#1E1E1E`) - cozy living room at night
- Per-persona accent colors: coral (`#D9645A`), sage (`#8ABF9F`), lavender (`#A89FCC`)
- Warm gold (`#E5A44A`) for primary actions and the human participant

### Typography
- System font stack with `'Noto Sans KR'` for Korean text
- Generous line-height (1.6) for hangul readability
- Monospace for λ values and numeric stats

### Accessibility
- Visible focus rings on all interactive elements (`outline: 2px solid #E5A44A`)
- `prefers-reduced-motion` media query disables all animations
- Color is never the sole indicator (text labels accompany gauge colors)
- Keyboard-navigable composer (Enter to send, Shift+Enter for newline)

### Responsive Design
- **Desktop**: Fixed 320px sidebar on the right
- **Mobile** (< 768px): Sidebar becomes a slide-in overlay toggled by a FAB. Composer takes full width.
- **Narrow**: Topic chips horizontally scroll; some metadata hidden

---

## What I'd Refine Next

1. **Gauge vertical mode**: On ultra-wide screens, consider vertical "thermometer" gauges that fill upward toward a θ line - more dramatic.
2. **Sound design**: Subtle audio cues for λ crossing θ (soft chime), silence (ambient room tone), human message (brief harmony).
3. **Avatar illustrations**: Replace the glyph system with proper illustrated character portraits that morph between expressions based on λ band.
4. **Message threading**: Visual lines connecting related utterances when the engine emits thread metadata.
5. **Historical replay**: A mode to replay past sessions with the same λ gauge animations, for reviewing conversation dynamics.

---

## Running Locally

```bash
npm install
npm run build
# Open dist/index.html in a browser, or:
npm run dev
```

No backend required - the mock engine runs entirely in the browser.
