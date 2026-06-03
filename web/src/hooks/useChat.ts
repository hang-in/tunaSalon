import { useState, useRef, useEffect, useCallback } from "react";
import type { ServerFrame, ChatMessage, EngineState, PersonaConfig } from "@/types";
import { connect as connectReal } from "@/lib/realEngine";
import { connect as connectMock } from "@/lib/mockEngine";

const HUMAN_PULSE_MS = 1200;

// VITE_MOCK=1 이면 mock 데모, 기본은 실 WebSocket 서버
const connect = import.meta.env.VITE_MOCK === "1" ? connectMock : connectReal;

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

export function useChat() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [engineState, setEngineState] = useState<EngineState>({
    intensities: { friend: 0.1, chaos: 0.15, summarizer: 0.05 },
    theta: 0.6,
    flow: 0.2,
    mu_scale: 1.0,
    pending: null,
    participants: [
      { id: "friend", name: "Friendly Regular" },
      { id: "chaos", name: "Grounded Realist" },
      { id: "summarizer", name: "Quiet Summarizer" },
      { id: "나", name: "나" },
    ],
    topics: ["부처님 오신날"],
  });
  const [connected, setConnected] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [humanPulse, setHumanPulse] = useState(false);
  const connRef = useRef<ReturnType<typeof connect> | null>(null);
  const msgIdRef = useRef(0);
  const pulseTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const conn = connect((frame: ServerFrame) => {
      if (frame.type === "state") {
        setEngineState({
          intensities: frame.intensities,
          theta: frame.theta,
          flow: frame.flow,
          mu_scale: frame.mu_scale,
          pending: frame.pending,
          participants: frame.participants,
          topics: frame.topics,
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
      }
    }, (isConnected: boolean) => setConnected(isConnected));
    connRef.current = conn;
    return () => conn.disconnect();
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

  const getPersonaConfig = useCallback((id: string): PersonaConfig => {
    return PERSONA_CONFIGS.find((p) => p.id === id) || PERSONA_CONFIGS[0];
  }, []);

  return {
    messages,
    engineState,
    connected,
    sidebarOpen,
    setSidebarOpen,
    sendMessage,
    sendTopics,
    getPersonaConfig,
    personaConfigs: PERSONA_CONFIGS,
    humanPulse,
  };
}
