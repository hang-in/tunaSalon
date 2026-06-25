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
let currentTopics = ["AI 규제와 오픈소스"];

const OPEN_SOURCE_AI_LINES: Record<string, string[]> = {
  friend: [
    "나는 오픈소스 AI를 막기보다 투명한 공개 기준을 세우는 쪽이 낫다고 봐. 모델 카드, 학습 데이터 범위, 위험 평가를 같이 열어두면 커뮤니티도 감시자가 될 수 있어.",
    "규제가 너무 넓으면 작은 연구팀과 개인 개발자가 먼저 위축될 것 같아. 고위험 배포에는 책임을 묻되, 연구와 검증 목적의 공개는 살려야 한다고 생각해.",
    "오픈소스는 문제를 숨기지 않고 함께 고치는 문화가 강점이야. 규제도 금지보다 감사 로그와 배포 책임을 요구하는 방식이면 협력 여지가 있어.",
  ],
  chaos: [
    "오픈소스라고 해서 자동으로 안전한 건 아니야. 가중치가 공개되면 악용 비용도 내려가니, 성능과 접근 범위에 따라 차등 규제를 두는 게 현실적이야.",
    "핵심은 공개 여부가 아니라 피해 가능성과 배포 규모야. 위험한 기능을 가진 모델은 오픈소스라도 red-team 결과와 완화 장치를 요구해야 해.",
    "너무 이상적으로 보면 안 돼. 기업은 책임 회피를 위해 오픈소스라고 포장할 수 있고, 국가는 안보 논리로 과도하게 막을 수 있으니 기준을 숫자로 좁혀야 해.",
  ],
  summarizer: [
    "정리하면 쟁점은 오픈소스를 허용하느냐가 아니라 어떤 위험부터 규제하느냐인 것 같아. 투명성, 악용 가능성, 배포 책임을 나눠 봐야 해.",
    "지금까지는 두 축이 보여. 하나는 공개 생태계를 살리는 기준이고, 다른 하나는 고위험 모델의 책임 있는 배포 장치야.",
    "결국 좋은 절충안은 저위험 연구 공개는 넓게 허용하고, 고성능 범용 모델에는 감사와 사후 책임을 붙이는 방향일 수 있어.",
  ],
};

const GENERIC_TOPIC_LINES: Record<string, (topic: string) => string[]> = {
  friend: (topic) => [
    `나는 "${topic}"에서 먼저 참여자들이 납득할 수 있는 공통 기준을 세우는 게 중요하다고 봐.`,
    `"${topic}"을 너무 금지 중심으로만 보면 현장에서 좋은 시도까지 같이 줄어들 수 있어.`,
  ],
  chaos: (topic) => [
    `"${topic}"은 의도보다 실행 기준이 더 중요해. 누가 책임지고 어떤 지표로 판단할지 정해야 해.`,
    `현실적으로 "${topic}"에는 비용과 부작용이 따라와. 원칙만으로는 부족하고 집행 가능성을 봐야 해.`,
  ],
  summarizer: (topic) => [
    `정리하면 "${topic}"의 핵심은 가치 판단과 실행 가능성 사이의 균형이야.`,
    `지금 논점은 "${topic}"을 넓게 허용할지, 위험 구간부터 좁게 관리할지로 모이는 것 같아.`,
  ],
};

const RECALL_NOTES: Record<string, string[]> = {
  friend: [
    "지난 대화에서: 공개 생태계는 신뢰와 검증 문화가 중요하다고 했어",
    "지난 대화에서: 작은 팀이 규제 비용을 감당하기 어렵다는 얘기가 나왔어",
  ],
  chaos: [
    "이전에: 고위험 기능은 배포 전 red-team이 필요하다고 했어",
    "과거 대화에서: 규제 기준은 숫자와 책임 주체가 있어야 한다고 했어",
  ],
  summarizer: [
    "이전 대화의 주제: 투명성과 안전성의 균형",
    "오늘 대화의 정리: 공개 범위와 배포 책임을 분리해서 보기",
  ],
};

function pickRandom<T>(arr: T[]): T {
  return arr[Math.floor(Math.random() * arr.length)];
}

function currentTopic(): string {
  return currentTopics[0] || "오늘의 토론 주제";
}

function topicAwareLine(speaker: string): string {
  const topic = currentTopic();
  const normalized = topic.replace(/\s+/g, "");
  if (normalized.includes("AI규제") || normalized.includes("오픈소스")) {
    return pickRandom(OPEN_SOURCE_AI_LINES[speaker] ?? OPEN_SOURCE_AI_LINES.friend);
  }
  return pickRandom(GENERIC_TOPIC_LINES[speaker]?.(topic) ?? GENERIC_TOPIC_LINES.friend(topic));
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
      room_id: "salon",
      intensities: { ...this.intensities },
      theta: THETA,
      flow: this.flow,
      mu_scale: this.mu_scale,
      liveliness: 0,
      pending: this.pending,
      participants: PARTICIPANTS,
      topics: [...currentTopics],
      paused: this.paused,
      tick_ms: 4000,
    };
  }

  tick(): ServerFrame[] {
    const frames: ServerFrame[] = [];

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
    const content = topicAwareLine(speaker);
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
    currentTopics = topics.map((topic) => topic.trim()).filter(Boolean).slice(0, 5);
  }
}

// ═─ Public API ─═══════════════════════════════════════════════════
let engine: ConversationEngine | null = null;
let intervalId: ReturnType<typeof setInterval> | null = null;
let callback: ((frame: ServerFrame) => void) | null = null;

export function connect(
  onFrame: (frame: ServerFrame) => void,
  onStatus?: (connected: boolean) => void,
  _roomId?: string,
  topics?: string[],
  _personas?: string[]
): { send: (frame: ClientFrame) => void; disconnect: () => void } {
  // Cleanup any previous connection
  if (intervalId) clearInterval(intervalId);
  if (topics?.length) currentTopics = topics;
  void _personas; // mock 엔진은 초기 참가자 스펙을 사용하지 않는다(시그니처 일치용).

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
        callback?.({
          type: "system",
          text: `토론 주제: ${currentTopics.join(", ") || "없음"}`,
        });
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
