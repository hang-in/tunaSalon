/**
 * ═══════════════════════════════════════════════════════════════════════════
 *  MOCK WEBSOCKET ENGINE  -  tunaSalon
 * ═══════════════════════════════════════════════════════════════════════════
 *
 *  Isolated simulation of the Rust conversation-flow backend.
 *  Swap: replace connect() with:
 *    const ws = new WebSocket('ws://localhost:PORT/ws');
 *    ws.onmessage = (e) => onFrame(JSON.parse(e.data));
 *    return { send: (f) => ws.send(JSON.stringify(f)) };
 */

import type { ServerFrame, ClientFrame, Participant } from "@/types";

const PARTICIPANTS: Participant[] = [
  { id: "friend", name: "Friendly Regular", model: "gemma4:31b-cloud" },
  { id: "chaos", name: "Grounded Realist", model: "gemma4:31b-cloud" },
  { id: "summarizer", name: "Quiet Summarizer", model: "qwen3.6-35b-fast" },
  { id: "나", name: "나" },
];

const THETA = 0.6;
const TOPICS_POOL = [
  "부처님 오신날",
  "주말 계획",
  "최근에 본 영화",
  "울릉도 독도",
  "커피 취향",
  "봄날의 산책",
  "좋아하는 음악",
  "일과 삶의 균형",
  "반려동물",
  "독서 습관",
];

let currentTopics = ["부처님 오신날"];
let tickCount = 0;

// ═─ Mock utterance bank (Korean) ─════════════════════════════════
const UTTERANCES: Record<string, string[][]> = {
  friend: [
    ["부처님 말씀 중에 집착을 버리라는 게 핵심인 것 같아.", "마음이 한결 가벼워지는 느낌이야."],
    ["오늘 날씨 진짜 좋지 않아?", "이런 날엔 꼭 나가서 산책해야 해."],
    ["나 요즘 새로운 카페 발견했어!", "분위기가 너무 아늑하고 좋더라고."],
    ["너도 등산 좋아한다고 했었잖아.", "이번 주말에 같이 가면 어떨까?"],
    ["봄향기가 참 좋은 것 같아.", "매년 이맘때면 기분이 설레."],
  ],
  chaos: [
    ["그건 좋은 관점이지만, 현실적으로 볼 때 좀 더 복잡하지 않을까?"],
    ["감정적으로 판단하기보다는 데이터를 보는 게 중요해.", "실제 수치가 말해주는 게 있거든."],
    ["좋은 생각이긴 한데 실행 가능성부터 따져봐야 해.", "자원과 시간이 충분한지 확인해야지."],
    ["너무 낙관적으로만 볼 필요는 없어.", "리스크도 함께 고려하면 좋겠어."],
    ["그건 개인의 경험일 뿐 일반화하긴 어려워.", "다양한 사례를 함께 볼 필요가 있어."],
  ],
  summarizer: [
    ["지금까지의 대화를 정리핶자면, 집착을 낮추는 게 공통된 화두인 것 같아."],
    ["서로 다른 관점이 나왔지만, 본질은 비슷하지 않나 싶어.", "현실성과 감정의 균형을 맞추는 거지."],
    ["이 대화의 핵심은 서로를 이해하려는 태도인 것 같아."],
    ["정리하면, 친구는 경험을, 현실주의자는 논리를, 나는 의미를 말하고 있어.", "흥미로운 조합이야."],
  ],
};

const RECALL_NOTES: Record<string, string[]> = {
  friend: [
    "지난 대화에서: 너 등산 좋아한다고 했지",
    "지난 대화에서: 새로운 카페를 찾고 있었어",
    "기억나? 봄에 벚꽃 구경 가자고 했잖아",
  ],
  chaos: [
    "이전에: 데이터 기반 의사결정을 중요하게 여긴다고 했어",
    "과거 대화에서: 시간 관리가 고민이었지",
  ],
  summarizer: [
    "이전 대화의 주제: 일과 삶의 균형",
    "오늘 아침 대화: 날씨와 기분의 상관관계",
  ],
};

function pickRandom<T>(arr: T[]): T {
  return arr[Math.floor(Math.random() * arr.length)];
}

// ═─ Conversation simulation ─═══════════════════════════════════════
class ConversationEngine {
  intensities: Record<string, number> = { friend: 0.1, chaos: 0.15, summarizer: 0.05 };
  flow = 0.2;
  mu_scale = 1.0;
  pending: string | null = null;
  isSpeaking = false;
  humanSpokeRecently = false;
  humanCooldown = 0;
  paused = false;

  getStateFrame(): ServerFrame {
    return {
      type: "state",
      intensities: { ...this.intensities },
      theta: THETA,
      flow: this.flow,
      mu_scale: this.mu_scale,
      pending: this.pending,
      participants: PARTICIPANTS,
      topics: [...currentTopics],
      paused: this.paused,
    };
  }

  tick(): ServerFrame[] {
    const frames: ServerFrame[] = [];
    tickCount++;

    // paused면 state frame만 emit(람다/발화 갱신 없음)
    if (this.paused) {
      frames.push(this.getStateFrame());
      return frames;
    }

    // Decay human-spoke-recently flag
    if (this.humanCooldown > 0) {
      this.humanCooldown--;
      if (this.humanCooldown === 0) this.humanSpokeRecently = false;
    }

    // If someone is generating, just send state update
    if (this.pending) {
      frames.push(this.getStateFrame());
      return frames;
    }

    // Update each persona's λ
    const ids = Object.keys(this.intensities);
    for (const id of ids) {
      if (this.isSpeaking) continue;

      let delta: number;
      if (this.humanSpokeRecently) {
        // Excited by human - faster rise
        delta = 0.04 + Math.random() * 0.06;
      } else {
        // Normal organic growth
        delta = 0.01 + Math.random() * 0.04;
      }

      // summarizer rises slower
      if (id === "summarizer") delta *= 0.5;

      // friend rises faster (more chatty)
      if (id === "friend") delta *= 1.2;

      this.intensities[id] = Math.min(0.95, this.intensities[id] + delta * this.mu_scale);
    }

    // Check if anyone crosses θ
    const ready = ids.filter((id) => this.intensities[id] >= THETA && !this.pending);

    if (ready.length > 0 && !this.isSpeaking) {
      // Pick the one with highest λ (or random if tied)
      ready.sort((a, b) => this.intensities[b] - this.intensities[a]);
      const speaker = ready[0];
      this.pending = speaker;
      this.isSpeaking = true;
      frames.push(this.getStateFrame());

      // Simulate "generation time" then emit utterance
      setTimeout(() => {
        this.emitUtterance(speaker);
      }, 1200 + Math.random() * 1500);
    } else {
      frames.push(this.getStateFrame());
    }

    // Random flow drift
    this.flow = Math.max(0, Math.min(1, this.flow + (Math.random() - 0.5) * 0.05));
    // mu_scale slowly decays toward 1.0
    this.mu_scale += (1.0 - this.mu_scale) * 0.02;

    return frames;
  }

  private emitUtterance(speaker: string) {
    const sentences = pickRandom(UTTERANCES[speaker]);
    const content = sentences.join(" ");
    const ts = Math.floor(Date.now() / 1000) % 86400;

    const utterance: ServerFrame = {
      type: "utterance",
      speaker,
      name: PARTICIPANTS.find((p) => p.id === speaker)?.name || speaker,
      content,
      ts,
    };

    // Occasionally emit a recall frame too
    if (Math.random() < 0.15 && RECALL_NOTES[speaker]) {
      const recall: ServerFrame = {
        type: "recall",
        speaker,
        note: pickRandom(RECALL_NOTES[speaker]),
      };
      this.deliver([recall, utterance]);
    } else {
      this.deliver([utterance]);
    }

    // Reset state
    this.intensities[speaker] = 0.08 + Math.random() * 0.05; // Drain to low
    // Excite others slightly
    for (const id of Object.keys(this.intensities)) {
      if (id !== speaker) {
        this.intensities[id] = Math.min(0.95, this.intensities[id] + 0.04 + Math.random() * 0.04);
      }
    }
    this.pending = null;
    this.isSpeaking = false;

    // Maybe change topics occasionally
    if (Math.random() < 0.08) {
      const newTopic = pickRandom(TOPICS_POOL);
      if (!currentTopics.includes(newTopic)) {
        currentTopics = [newTopic, ...currentTopics].slice(0, 3);
        const sys: ServerFrame = {
          type: "system",
          text: `화제가 '${newTopic}'로 바뀌었습니다`,
        };
        this.deliver([sys, this.getStateFrame()]);
        return;
      }
    }

    this.deliver([this.getStateFrame()]);
  }

  deliver: (frames: ServerFrame[]) => void = () => {};

  onHumanMessage() {
    // All personas get excited - spike their λ
    for (const id of Object.keys(this.intensities)) {
      this.intensities[id] = Math.min(0.92, this.intensities[id] + 0.35 + Math.random() * 0.15);
    }
    this.humanSpokeRecently = true;
    this.humanCooldown = 5; // ticks
    this.mu_scale = 1.0; // Reset cooling
  }

  setTopics(topics: string[]) {
    currentTopics = topics.slice(0, 5);
  }
}

// ═─ Public API ─═══════════════════════════════════════════════════
let engine: ConversationEngine | null = null;
let intervalId: ReturnType<typeof setInterval> | null = null;
let callback: ((frame: ServerFrame) => void) | null = null;

export function connect(
  onFrame: (frame: ServerFrame) => void,
  onStatus?: (connected: boolean) => void
): { send: (frame: ClientFrame) => void; disconnect: () => void } {
  // Cleanup any previous connection
  if (intervalId) clearInterval(intervalId);

  engine = new ConversationEngine();
  callback = onFrame;

  engine.deliver = (frames) => {
    for (const f of frames) onFrame(f);
  };

  // mock은 즉시 연결됨
  onStatus?.(true);

  // Initial state burst
  setTimeout(() => {
    onFrame(engine!.getStateFrame());
  }, 100);

  // Tick every 800-1200ms with slight jitter
  const scheduleTick = () => {
    const delay = 800 + Math.random() * 400;
    intervalId = setTimeout(() => {
      if (!engine) return;
      const frames = engine.tick();
      for (const f of frames) onFrame(f);
      scheduleTick();
    }, delay);
  };
  scheduleTick();

  return {
    send(frame: ClientFrame) {
      if (!engine) return;
      if (frame.type === "message") {
        engine.onHumanMessage();
        // Echo the message back as an utterance from "나"
        const humanUtterance: ServerFrame = {
          type: "utterance",
          speaker: "나",
          name: "나",
          content: frame.text,
          ts: Math.floor(Date.now() / 1000) % 86400,
        };
        callback?.(humanUtterance);
        // Then immediately send updated state
        setTimeout(() => callback?.(engine!.getStateFrame()), 50);
      } else if (frame.type === "topic") {
        engine.setTopics(frame.topics);
        callback?.(engine.getStateFrame());
      } else if (frame.type === "pause") {
        engine.paused = frame.paused;
        callback?.(engine.getStateFrame()); // 즉시 paused 상태 반영
      }
    },
    disconnect() {
      if (intervalId) clearTimeout(intervalId);
      engine = null;
      callback = null;
      onStatus?.(false);
    },
  };
}
