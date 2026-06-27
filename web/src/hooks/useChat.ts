import { useState, useRef, useEffect, useCallback } from "react";
import type { ServerFrame, ChatMessage, EngineState, PersonaConfig } from "@/types";
import { connect as connectReal } from "@/lib/realEngine";
import { connect as connectMock } from "@/lib/mockEngine";
import { personaColorSet } from "@/lib/personaAvatar";

const HUMAN_PULSE_MS = 1200;

// VITE_MOCK=1 이면 mock 데모, 기본은 실 WebSocket 서버
const connect = import.meta.env.VITE_MOCK === "1" ? connectMock : connectReal;

const DEFAULT_ENGINE_STATE: EngineState = {
  room_id: "salon",
  intensities: { friend: 0.1, chaos: 0.15, summarizer: 0.05 },
  theta: 0.6,
  flow: 0.2,
  mu_scale: 1.0,
  liveliness: 0,
  pending: null,
  participants: [
    { id: "friend", name: "Friendly Regular" },
    { id: "chaos", name: "Grounded Realist" },
    { id: "summarizer", name: "Quiet Summarizer" },
    { id: "나", name: "나" },
  ],
  topics: [],
  paused: false,
  tick_ms: 6000,
};

const PERSONA_CONFIGS: PersonaConfig[] = [
  {
    id: "friend",
    name: "Friendly Regular",
    color: "#D9645A",
    glowColor: "rgba(217, 100, 90, 0.5)",
    bgColor: "rgba(217, 100, 90, 0.12)",
    description: "따뜻하고 친근한",
  },
  {
    id: "chaos",
    name: "Grounded Realist",
    color: "#8ABF9F",
    glowColor: "rgba(138, 191, 159, 0.5)",
    bgColor: "rgba(138, 191, 159, 0.12)",
    description: "현실적이고 차분한",
  },
  {
    id: "summarizer",
    name: "Quiet Summarizer",
    color: "#A89FCC",
    glowColor: "rgba(168, 159, 204, 0.5)",
    bgColor: "rgba(168, 159, 204, 0.12)",
    description: "조용히 정리하는",
  },
  {
    id: "나",
    name: "나",
    color: "#E5A44A",
    glowColor: "rgba(229, 164, 74, 0.5)",
    bgColor: "rgba(229, 164, 74, 0.12)",
    description: "당신",
  },
];

interface UseChatOptions {
  enabled?: boolean;
  roomId?: string;
  topics?: string[];
  /** 새 방 수동 구성 참가자 ["blood:mbti:zodiac:role", ...]. 비면 서버가 랜덤 3명을 시딩. */
  personas?: string[];
  /** 페르소나가 쓸 모델 태그(최대 3). 새 방 시딩에만 적용. */
  models?: string[];
}

export function useChat({ enabled = true, roomId, topics = [], personas, models }: UseChatOptions = {}) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [engineState, setEngineState] = useState<EngineState>(DEFAULT_ENGINE_STATE);
  const [connected, setConnected] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [humanPulse, setHumanPulse] = useState(false);
  const connRef = useRef<ReturnType<typeof connect> | null>(null);
  const msgIdRef = useRef(0);
  const pulseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const topicKey = topics.join("\u0000");

  useEffect(() => {
    if (!enabled) {
      return;
    }
    const conn = connect((frame: ServerFrame) => {
      if (frame.type === "state") {
        setEngineState({
          room_id: frame.room_id,
          intensities: frame.intensities,
          theta: frame.theta,
          flow: frame.flow,
          mu_scale: frame.mu_scale,
          liveliness: frame.liveliness ?? 0,
          pending: frame.pending,
          participants: frame.participants,
          topics: frame.topics,
          paused: frame.paused ?? false,
          tick_ms: frame.tick_ms ?? 6000,
        });
        setMessages((prev) => {
          const hasTranscript = prev.some((msg) => msg.type === "utterance");
          if (hasTranscript || !frame.messages?.length) return prev;
          const systemMessages = prev.filter((msg) => msg.type === "system");
          const historyMessages: ChatMessage[] = frame.messages.map((msg, index) => ({
            id: `history-${frame.room_id}-${index}`,
            type: "utterance",
            speaker: msg.speaker,
            name: msg.name,
            content: msg.content,
            ts: msg.ts,
            isHuman: msg.speaker === "나",
          }));
          return [...systemMessages, ...historyMessages];
        });
        setConnected(true);
      } else if (frame.type === "utterance") {
        msgIdRef.current++;
        setMessages((prev) => [
          ...prev,
          {
            id: `msg-${msgIdRef.current}`,
            type: "utterance",
            speaker: frame.speaker,
            name: frame.name,
            content: frame.content,
            ts: frame.ts,
            isHuman: frame.speaker === "나",
          },
        ]);
      } else if (frame.type === "recall") {
        msgIdRef.current++;
        setMessages((prev) => [
          ...prev,
          {
            id: `msg-${msgIdRef.current}`,
            type: "recall",
            speaker: frame.speaker,
            name: frame.speaker,
            content: frame.note,
            ts: 0,
            isHuman: false,
          },
        ]);
      } else if (frame.type === "system") {
        msgIdRef.current++;
        setMessages((prev) => [
          ...prev,
          {
            id: `msg-${msgIdRef.current}`,
            type: "system",
            speaker: "system",
            name: "system",
            content: frame.text,
            ts: 0,
            isHuman: false,
          },
        ]);
      } else if (frame.type === "report") {
        msgIdRef.current++;
        setMessages((prev) => [
          ...prev,
          {
            id: `msg-${msgIdRef.current}`,
            type: "report",
            speaker: "report",
            name: "토론 리포트",
            content: frame.text,
            ts: 0,
            isHuman: false,
          },
        ]);
      }
    }, (isConnected: boolean) => setConnected(isConnected), roomId, topics, personas, models);
    connRef.current = conn;
    return () => {
      conn.disconnect();
      connRef.current = null;
      setConnected(false);
    };
    // personas는 방 생성 시점에만 의미가 있어 의존성에서 제외(이후 변경은 서버 무시).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, roomId, topicKey, topics]);

  const resetChat = useCallback(() => {
    msgIdRef.current = 0;
    setMessages([]);
    setEngineState(DEFAULT_ENGINE_STATE);
    setConnected(false);
  }, []);

  const triggerHumanPulse = useCallback(() => {
    setHumanPulse(true);
    if (pulseTimerRef.current) clearTimeout(pulseTimerRef.current);
    pulseTimerRef.current = setTimeout(() => setHumanPulse(false), HUMAN_PULSE_MS);
  }, []);

  const sendMessage = useCallback((text: string) => {
    if (!text.trim() || !connRef.current) return;
    connRef.current.send({ type: "message", text: text.trim() });
    triggerHumanPulse();
  }, [triggerHumanPulse]);

  const sendTopics = useCallback((topics: string[]) => {
    if (!connRef.current) return;
    connRef.current.send({ type: "topic", topics });
  }, []);

  const sendPause = useCallback((paused: boolean) => {
    if (!connRef.current) return;
    connRef.current.send({ type: "pause", paused });
  }, []);

  const sendInvite = useCallback((blood: string, mbti: string, zodiac: string, role?: string) => {
    if (!connRef.current) return;
    const frame = role
      ? { type: "invite" as const, blood, mbti, zodiac, role }
      : { type: "invite" as const, blood, mbti, zodiac };
    connRef.current.send(frame);
  }, []);

  const sendRemove = useCallback((id: string) => {
    if (!connRef.current) return;
    connRef.current.send({ type: "remove", id });
  }, []);

  const sendHumanProfile = useCallback(
    (blood: string, mbti: string, zodiac: string, role: string) => {
      if (!connRef.current) return;
      connRef.current.send({ type: "human_profile", blood, mbti, zodiac, role });
    },
    [],
  );

  const sendPace = useCallback((intervalMs: number) => {
    if (!connRef.current) return;
    connRef.current.send({ type: "pace", interval_ms: intervalMs });
  }, []);

  const sendReset = useCallback((topics: string[]) => {
    if (!connRef.current) return;
    msgIdRef.current = 0;
    setMessages([]);
    connRef.current.send({ type: "reset", topics });
  }, []);

  const getPersonaConfig = useCallback((id: string, blood?: string): PersonaConfig => {
    const found = PERSONA_CONFIGS.find((p) => p.id === id);
    if (found) return found;
    // 동적 persona(한글 slug)는 하드코딩 목록에 없다. 혈액형 팔레트(아바타와 동일) 우선,
    // 없으면 id 해시로 결정적 색을 배정한다(사이드바 게이지·카드·아바타 색 통일).
    return { id, name: id, ...personaColorSet(id, blood), description: "" };
  }, []);

  return {
    messages,
    engineState,
    connected,
    sidebarOpen,
    setSidebarOpen,
    sendMessage,
    sendTopics,
    sendPause,
    sendPace,
    sendReset,
    sendInvite,
    sendRemove,
    sendHumanProfile,
    getPersonaConfig,
    personaConfigs: PERSONA_CONFIGS,
    humanPulse,
    resetChat,
  };
}
