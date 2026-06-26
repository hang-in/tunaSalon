// ── Data contract: server → client ──────────────────────────────

export interface Participant {
  id: string;
  name: string;
  model?: string;
  axes?: {
    blood: string;
    mbti: string;
    zodiac: string;
    role: string;
  };
}

export interface StateFrame {
  type: "state";
  room_id: string;
  intensities: Record<string, number>;
  theta: number;
  flow: number;
  mu_scale: number;
  liveliness: number;
  pending: string | null;
  participants: Participant[];
  messages?: {
    speaker: string;
    name: string;
    content: string;
    ts: number;
  }[];
  topics: string[];
  paused: boolean;
  tick_ms: number;
}

export interface UtteranceFrame {
  type: "utterance";
  speaker: string;
  name: string;
  content: string;
  ts: number;
}

export interface RecallFrame {
  type: "recall";
  speaker: string;
  note: string;
}

export interface SystemFrame {
  type: "system";
  text: string;
}

export type ServerFrame = StateFrame | UtteranceFrame | RecallFrame | SystemFrame;

// ── Data contract: client → server ──────────────────────────────

export interface ClientMessageFrame {
  type: "message";
  text: string;
}

export interface ClientTopicFrame {
  type: "topic";
  topics: string[];
}

export interface ClientPauseFrame {
  type: "pause";
  paused: boolean;
}

export interface ClientInviteFrame {
  type: "invite";
  blood: string;
  mbti: string;
  zodiac: string;
  role?: string;
}

export interface ClientRemoveFrame {
  type: "remove";
  id: string;
}

export interface ClientPaceFrame {
  type: "pace";
  interval_ms: number;
}

export interface ClientResetFrame {
  type: "reset";
  topics: string[];
}

export interface ClientHumanProfileFrame {
  type: "human_profile";
  blood: string;
  mbti: string;
  zodiac: string;
  role: string;
}

export type ClientFrame = ClientMessageFrame | ClientTopicFrame | ClientPauseFrame | ClientPaceFrame | ClientInviteFrame | ClientRemoveFrame | ClientResetFrame | ClientHumanProfileFrame;

// ── UI-local types ──────────────────────────────────────────────

export interface PersonaConfig {
  id: string;
  name: string;
  color: string;
  glowColor: string;
  bgColor: string;
  description: string;
}

export interface ChatMessage {
  id: string;
  type: "utterance" | "recall" | "system";
  speaker: string;
  name: string;
  content: string;
  ts: number;
  isHuman: boolean;
}

export interface EngineState {
  room_id: string;
  intensities: Record<string, number>;
  theta: number;
  flow: number;
  mu_scale: number;
  liveliness: number;
  pending: string | null;
  participants: Participant[];
  topics: string[];
  paused: boolean;
  tick_ms: number;
}
