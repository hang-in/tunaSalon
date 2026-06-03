// ── Data contract: server → client ──────────────────────────────

export interface Participant {
  id: string;
  name: string;
  model?: string;
}

export interface StateFrame {
  type: "state";
  intensities: Record<string, number>;
  theta: number;
  flow: number;
  mu_scale: number;
  pending: string | null;
  participants: Participant[];
  topics: string[];
  paused: boolean;
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

export type ClientFrame = ClientMessageFrame | ClientTopicFrame | ClientPauseFrame;

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
  intensities: Record<string, number>;
  theta: number;
  flow: number;
  mu_scale: number;
  pending: string | null;
  participants: Participant[];
  topics: string[];
  paused: boolean;
}
