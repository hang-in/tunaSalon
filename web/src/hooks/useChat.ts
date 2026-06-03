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

// 동적 초대 persona(하드코딩 목록에 없음)에 id 해시로 배정하는 색 팔레트.
const FALLBACK_PALETTE = [
  { color: "#7BA7D9", glowColor: "rgba(123, 167, 217, 0.5)", bgColor: "rgba(123, 167, 217, 0.12)" },
  { color: "#D99A5B", glowColor: "rgba(217, 154, 91, 0.5)", bgColor: "rgba(217, 154, 91, 0.12)" },
  { color: "#9FCC8A", glowColor: "rgba(159, 204, 138, 0.5)", bgColor: "rgba(159, 204, 138, 0.12)" },
  { color: "#CC8AB8", glowColor: "rgba(204, 138, 184, 0.5)", bgColor: "rgba(204, 138, 184, 0.12)" },
  { color: "#5BC0BE", glowColor: "rgba(91, 192, 190, 0.5)", bgColor: "rgba(91, 192, 190, 0.12)" },
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
    paused: false,
    tick_ms: 4000,
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
          paused: frame.paused ?? false,
          tick_ms: frame.tick_ms ?? 4000,
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

  const sendPace = useCallback((intervalMs: number) => {
    if (!connRef.current) return;
    connRef.current.send({ type: "pace", interval_ms: intervalMs });
  }, []);

  const getPersonaConfig = useCallback((id: string): PersonaConfig => {
    const found = PERSONA_CONFIGS.find((p) => p.id === id);
    if (found) return found;
    // 동적 초대 persona(id가 한글 slug)는 하드코딩 목록에 없다.
    // PERSONA_CONFIGS[0]로 폴백하면 모두 같은 이름/색이 되므로(전부 "Friendly Regular"),
    // id 해시로 결정적 색을 배정한다. 이름은 호출처가 message.name/participant.name을 쓴다.
    let hash = 0;
    for (const ch of id) hash = (hash + ch.charCodeAt(0)) % FALLBACK_PALETTE.length;
    const pal = FALLBACK_PALETTE[hash];
    return { id, name: id, ...pal, description: "" };
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
    sendInvite,
    sendRemove,
    getPersonaConfig,
    personaConfigs: PERSONA_CONFIGS,
    humanPulse,
  };
}
