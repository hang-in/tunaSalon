import { useRef, useEffect, useMemo } from "react";
import { MessageCircle, Sparkles, UserPlus, UserMinus, FileText } from "lucide-react";
import type { ChatMessage, EngineState, PersonaConfig } from "@/types";
import { bloodLabel, zodiacLabel } from "@/lib/personaLabels";
import { RichText } from "@/components/RichText";
import { PersonaAvatar, poseFromLambda } from "@/lib/personaAvatar";
import { personaDescription } from "@/lib/personaDescription";

interface ChatAreaProps {
  messages: ChatMessage[];
  engineState: EngineState;
  getPersonaConfig: (id: string) => PersonaConfig;
  connected: boolean;
}

export function ChatArea({ messages, engineState, getPersonaConfig, connected }: ChatAreaProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    if (bottomRef.current) {
      bottomRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [messages.length]);

  // Check if all λ are below θ (silence mode)
  const allBelowTheta = useMemo(() => {
    const ids = Object.keys(engineState.intensities).filter((id) => id !== "나");
    return ids.every((id) => engineState.intensities[id] < engineState.theta);
  }, [engineState.intensities, engineState.theta]);
  const pendingName = useMemo(() => {
    if (!engineState.pending) return null;
    return engineState.participants.find((p) => p.id === engineState.pending)?.name ?? engineState.pending;
  }, [engineState.participants, engineState.pending]);

  // 메시지 본문에서 강조할 참가자 닉네임 + 색상(사람 "나"는 제외 — 흔한 토큰 오탐 방지).
  const mentions = useMemo(
    () =>
      engineState.participants
        .filter((p) => p.id !== "나")
        .map((p) => ({ name: p.name, color: getPersonaConfig(p.id).color })),
    [engineState.participants, getPersonaConfig]
  );

  // Group consecutive messages from same speaker
  const grouped = useMemo(() => {
    const groups: { speaker: string; messages: ChatMessage[] }[] = [];
    for (const msg of messages) {
      if (msg.type === "system" || msg.type === "recall" || msg.type === "report") {
        groups.push({ speaker: "_special_", messages: [msg] });
        continue;
      }
      const last = groups[groups.length - 1];
      if (last && last.speaker === msg.speaker && msg.type === "utterance") {
        last.messages.push(msg);
      } else {
        groups.push({ speaker: msg.speaker, messages: [msg] });
      }
    }
    return groups;
  }, [messages]);

  return (
    <div
      ref={scrollRef}
      className="flex-1 overflow-y-auto relative"
      style={{ scrollBehavior: "smooth" }}
    >
      {/* Quiet mode vignette overlay */}
      {allBelowTheta && messages.length > 2 && (
        <div className="absolute inset-0 z-10 quiet-vignette pointer-events-none" />
      )}

      <div className="flex flex-col gap-1 px-4 lg:px-6 py-6">
        {/* 연결 전 로딩 상태 */}
        {messages.length === 0 && !connected && (
          <div className="flex flex-col items-center justify-center py-20 text-center">
            <div
              className="w-16 h-16 rounded-2xl flex items-center justify-center mb-5"
              style={{ background: "rgba(229, 164, 74, 0.06)" }}
            >
              <span
                className="w-8 h-8 rounded-full border-2 border-[var(--accent-warm)] border-t-transparent animate-spin"
                aria-hidden="true"
              />
            </div>
            <h2 className="text-base font-semibold text-[var(--text-secondary)] mb-2">
              엔진에 연결 중...
            </h2>
            <p className="text-xs text-[var(--text-secondary)] opacity-60">
              서버와 연결을 맺고 있습니다
            </p>
          </div>
        )}

        {/* Welcome state (연결 후, 메시지 없을 때) */}
        {messages.length === 0 && connected && (
          <div className="flex flex-col items-center justify-center py-20 text-center">
            <div
              className="w-16 h-16 rounded-2xl flex items-center justify-center mb-5"
              style={{ background: "rgba(229, 164, 74, 0.1)" }}
            >
              {engineState.pending ? (
                <span
                  className="w-8 h-8 rounded-full border-2 border-[var(--accent-warm)] border-t-transparent animate-spin"
                  aria-hidden="true"
                />
              ) : (
                <MessageCircle size={28} style={{ color: "var(--accent-warm)" }} />
              )}
            </div>
            <h2 className="text-lg font-bold text-[var(--text-primary)] mb-2">
              {engineState.pending ? "첫 발화 생성 중" : "토론방에 입장했습니다"}
            </h2>
            <p className="text-sm text-[var(--text-secondary)] max-w-sm leading-relaxed break-keep">
              {engineState.topics.length > 0
                ? `주제: ${engineState.topics.join(", ")}`
                : "아직 주제가 없습니다."}
              <br />
              {pendingName
                ? `${pendingName}의 응답을 기다리는 중입니다.`
                : "곧 첫 참가자가 입장을 밝힙니다."}
            </p>
            <div className="flex items-center gap-2 mt-6">
              <div className="w-2 h-2 rounded-full pulse-dot" style={{ background: "#4ade80" }} />
              <span className="text-xs text-[var(--text-secondary)]">
                {engineState.pending ? "LLM 생성 중" : "토론 엔진 가동 중"}
              </span>
            </div>
          </div>
        )}

        {/* Messages */}
        {grouped.map((group, gi) => {
          if (group.speaker === "_special_") {
            return group.messages.map((msg) => (
              <SpecialMessage key={msg.id} message={msg} />
            ));
          }

          const isHuman = group.messages[0]?.isHuman ?? false;
          const config = getPersonaConfig(group.speaker);
          // 화자 표시 이름: 서버가 보낸 message.name(동적 persona 실제 이름) 우선.
          // config.name은 하드코딩 3명만 정확하고 동적 persona는 폴백이라 쓰지 않는다.
          const displayName = group.messages[0]?.name || config.name;

          // 이 speaker의 마지막 그룹인지 판정(pending 깜빡임을 마지막 그룹에만 적용).
          const isLastGroupOfSpeaker =
            gi === grouped.map((g) => g.speaker).lastIndexOf(group.speaker);

          // 이 participant의 axes 정보 (동적 persona만 존재).
          const participantAxes = engineState.participants.find(
            (p) => p.id === group.speaker
          )?.axes;

          return (
            <div
              key={gi}
              className={`flex gap-3 mb-1 ${isHuman ? "flex-row-reverse" : "flex-row"}`}
            >
              {/* Avatar */}
              <div
                className={`shrink-0 w-10 h-10 rounded-full flex items-center justify-center text-sm font-bold select-none overflow-hidden transition-transform duration-300 ${
                  engineState.pending === group.speaker && isLastGroupOfSpeaker ? "scale-110" : ""
                }`}
                style={{
                  background: `${config.color}22`,
                  color: config.color,
                  boxShadow:
                    engineState.pending === group.speaker && isLastGroupOfSpeaker
                      ? `0 0 12px ${config.glowColor}`
                      : "none",
                }}
              >
                {isHuman ? (
                  "나"
                ) : (
                  <PersonaAvatar
                    axes={participantAxes}
                    color={config.color}
                    pose={poseFromLambda(
                      engineState.intensities[group.speaker] ?? 0,
                      engineState.theta,
                      engineState.pending === group.speaker && isLastGroupOfSpeaker,
                    )}
                    size={40}
                  />
                )}
              </div>

              {/* Bubble(s) */}
              <div className={`flex flex-col ${isHuman ? "items-end" : "items-start"} max-w-[75%] sm:max-w-[65%]`}>
                {/* 사람이 캐릭터를 만들면 본인 이름도 표시(기본 "나"는 생략) */}
                {isHuman && displayName !== "나" && (
                  <span className="mb-1 mr-1 text-[11px] font-medium text-[var(--text-secondary)]">
                    {displayName}
                  </span>
                )}
                {/* Name label + axes 배지 */}
                {!isHuman && (
                  <span className="flex items-baseline gap-1.5 mb-1 ml-1">
                    <span
                      className="text-[11px] font-medium text-[var(--text-secondary)] cursor-help"
                      title={personaDescription(participantAxes)}
                    >
                      {displayName}
                    </span>
                    {participantAxes && (
                      <>
                        {/* 데스크탑: MBTI · 혈액형 · 별자리 · 역할 */}
                        <span
                          className="hidden sm:inline text-[10px] text-[var(--text-secondary)] opacity-60"
                        >
                          {participantAxes.mbti} · {bloodLabel(participantAxes.blood)} · {zodiacLabel(participantAxes.zodiac)}
                        </span>
                        {/* 모바일: MBTI · 혈액형 */}
                        <span
                          className="inline sm:hidden text-[10px] text-[var(--text-secondary)] opacity-60"
                        >
                          {participantAxes.mbti}·{participantAxes.blood}
                        </span>
                      </>
                    )}
                  </span>
                )}

                {group.messages.map((msg, mi) => (
                  <div
                    key={msg.id}
                    className={`msg-enter mb-1 px-4 py-2.5 text-[15px] leading-relaxed ${
                      isHuman
                        ? "rounded-2xl rounded-br-sm"
                        : "rounded-2xl rounded-bl-sm"
                    }`}
                    style={{
                      background: isHuman ? "var(--bg-elevated)" : "var(--bg-surface)",
                      color: "var(--text-primary)",
                      animationDelay: `${mi * 0.05}s`,
                    }}
                  >
                    {isHuman ? (
                      msg.content
                    ) : (
                      <RichText content={msg.content} mentions={mentions} />
                    )}
                  </div>
                ))}
                {/* 생각중(...) 표시는 사이드바 "대화 엔진 상태" 카드로 이동(PersonaPresence). */}
              </div>
            </div>
          );
        })}

        <div ref={bottomRef} />
      </div>
    </div>
  );
}

function SpecialMessage({ message }: { message: ChatMessage }) {
  if (message.type === "report") {
    return (
      <div className="my-4">
        <div
          className="rounded-xl overflow-hidden"
          style={{ border: "1px solid var(--accent-warm)", background: "var(--bg-surface)" }}
        >
          <div
            className="flex items-center gap-2 px-4 py-2.5"
            style={{ background: "rgba(229, 164, 74, 0.12)", color: "var(--accent-warm)" }}
          >
            <FileText size={15} />
            <span className="text-[13px] font-bold">토론 리포트</span>
          </div>
          <div
            className="px-4 py-3 text-[13.5px] leading-relaxed whitespace-pre-wrap"
            style={{ color: "var(--text-primary)" }}
          >
            {message.content}
          </div>
        </div>
      </div>
    );
  }
  if (message.type === "system") {
    const text = message.content;
    const isJoin = text.includes("입장");
    const isLeave = text.includes("나갔");

    if (isJoin) {
      return (
        <div className="flex justify-center my-3">
          <span
            className="flex items-center gap-1.5 text-[13px] font-medium px-4 py-1.5 rounded-full"
            style={{
              color: "#4ade80",
              background: "rgba(74, 222, 128, 0.08)",
              border: "1px solid rgba(74, 222, 128, 0.2)",
            }}
          >
            <UserPlus size={13} />
            {text}
          </span>
        </div>
      );
    }

    if (isLeave) {
      return (
        <div className="flex justify-center my-3">
          <span
            className="flex items-center gap-1.5 text-[13px] font-medium px-4 py-1.5 rounded-full"
            style={{
              color: "var(--text-secondary)",
              background: "var(--bg-surface)",
              border: "1px solid var(--border-color)",
            }}
          >
            <UserMinus size={13} />
            {text}
          </span>
        </div>
      );
    }

    // 그 외 system (화제 변경 등): 기존 스타일
    return (
      <div className="flex justify-center my-3">
        <span
          className="text-xs italic px-4 py-1.5 rounded-full"
          style={{ color: "var(--text-secondary)", background: "var(--bg-surface)" }}
        >
          {text}
        </span>
      </div>
    );
  }

  if (message.type === "recall") {
    return (
      <div className="flex justify-center my-2">
        <div
          className="flex items-center gap-2 text-xs px-4 py-2 rounded-xl max-w-md"
          style={{
            color: "var(--text-secondary)",
            background: "rgba(168, 159, 204, 0.08)",
            border: "1px dashed rgba(168, 159, 204, 0.25)",
          }}
        >
          <Sparkles size={12} style={{ color: "var(--accent-lavender)" }} />
          <span className="italic">{message.content}</span>
        </div>
      </div>
    );
  }

  return null;
}
