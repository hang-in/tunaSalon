import { useMemo } from "react";
import { Activity, Thermometer, Wind, X, UserMinus, Users, Zap } from "lucide-react";
import { PersonaPresence } from "./PersonaPresence";
import { InvitePanel } from "./InvitePanel";
import type { EngineState, PersonaConfig } from "@/types";

interface SidePanelProps {
  engineState: EngineState;
  personaConfigs: PersonaConfig[];
  open: boolean;
  onClose: () => void;
  humanPulse?: boolean;
  onInvite?: (blood: string, mbti: string, zodiac: string, role?: string) => void;
  onRemove?: (id: string) => void;
  onPace?: (intervalMs: number) => void;
}

export function SidePanel({ engineState, personaConfigs, open, onClose, humanPulse = false, onInvite, onRemove, onPace }: SidePanelProps) {
  // Compute silence status
  const isSilent = useMemo(() => {
    const ids = Object.keys(engineState.intensities).filter((id) => id !== "나");
    return ids.every((id) => engineState.intensities[id] < engineState.theta);
  }, [engineState.intensities, engineState.theta]);

  // Liveliness description
  const livelinessDesc = useMemo(() => {
    const l = engineState.liveliness;
    if (l > 0.7) return { text: "들썩들썩", color: "#E5A44A" };
    if (l > 0.4) return { text: "도란도란", color: "#D9A05B" };
    if (l > 0.15) return { text: "차분", color: "#A89FCC" };
    return { text: "잠잠", color: "var(--text-secondary)" };
  }, [engineState.liveliness]);

  // Flow description
  const flowDesc = useMemo(() => {
    const f = engineState.flow;
    if (f < 0.25) return { text: "활발한 대화", color: "#8ABF9F" };
    if (f < 0.5) return { text: "다양한 주제", color: "#E5A44A" };
    if (f < 0.75) return { text: "정리되는 중", color: "#A89FCC" };
    return { text: "한 주제로 수렴", color: "#D9645A" };
  }, [engineState.flow]);

  // mu_scale description
  const muDesc = useMemo(() => {
    const m = engineState.mu_scale;
    if (m > 0.85) return { text: "활기참" };
    if (m > 0.65) return { text: "적정" };
    if (m > 0.45) return { text: "냉각 중" };
    return { text: "조용함" };
  }, [engineState.mu_scale]);

  // mu_scale bar width: 0.4~1.0 범위를 0~100%로 매핑
  const muPct = Math.max(0, Math.min(1, (engineState.mu_scale - 0.4) / 0.6));

  return (
    <>
      {/* Mobile overlay backdrop */}
      {open && (
        <div
          className="fixed inset-0 z-40 bg-black/40 lg:hidden"
          onClick={onClose}
        />
      )}

      <aside
        className={`
          fixed top-16 right-0 bottom-0 z-50 w-80
          transform transition-transform duration-300 ease-out
          lg:translate-x-0 lg:static lg:z-auto lg:h-[calc(100vh-64px)]
          ${open ? "translate-x-0" : "translate-x-full"}
        `}
        style={{
          background: "var(--bg-surface)",
          borderLeft: "1px solid var(--border-color)",
        }}
      >
        <div className="h-full overflow-y-auto p-5">
          {/* Mobile close */}
          <button
            onClick={onClose}
            className="lg:hidden absolute top-3 right-3 p-1.5 rounded-lg hover:bg-white/5 transition-colors"
          >
            <X size={16} className="text-[var(--text-secondary)]" />
          </button>

          {/* Section: Personas */}
          <div className="mb-6">
            <div className="flex items-center gap-2 mb-4">
              <Activity size={14} style={{ color: "var(--accent-warm)" }} />
              <h2 className="text-[13px] font-bold uppercase tracking-wider text-[var(--text-secondary)]">
                대화 엔진 상태
              </h2>
            </div>

            <div className="flex flex-col gap-2">
              {/* 서버 participants 기준 렌더(human 제외) */}
              {engineState.participants
                .filter((p) => p.id !== "나")
                .map((participant) => {
                  // PersonaConfig 매칭: 서버가 동적으로 추가한 persona는 fallback config 생성
                  const config = personaConfigs.find((c) => c.id === participant.id) ?? {
                    id: participant.id,
                    name: participant.name,
                    color: "#A89FCC",
                    glowColor: "rgba(168, 159, 204, 0.5)",
                    bgColor: "rgba(168, 159, 204, 0.12)",
                    description: participant.model ?? "",
                  };
                  const displayConfig = {
                    ...config,
                    name: participant.name,
                    description: participant.model ?? config.description,
                  };
                  return (
                    <div key={participant.id} className="relative group">
                      <PersonaPresence
                        config={displayConfig}
                        lambda={engineState.intensities[participant.id] ?? 0}
                        theta={engineState.theta}
                        isPending={engineState.pending === participant.id}
                        model={participant.model}
                        humanPulse={humanPulse}
                        axes={participant.axes}
                      />
                      {/* Remove 버튼: persona 카드 우상단, 호버 시 표시 */}
                      {onRemove && (
                        <button
                          onClick={() => onRemove(participant.id)}
                          className="absolute top-1.5 right-1.5 p-1 rounded-md opacity-0 group-hover:opacity-100 transition-opacity"
                          style={{
                            background: "rgba(217, 100, 90, 0.15)",
                            color: "#D9645A",
                          }}
                          aria-label={`${participant.name} 내보내기`}
                          title={`${participant.name} 내보내기`}
                        >
                          <UserMinus size={11} />
                        </button>
                      )}
                    </div>
                  );
                })}

              {/* Human card */}
              <PersonaPresence
                config={personaConfigs.find((p) => p.id === "나") ?? {
                  id: "나",
                  name: "나",
                  color: "#E5A44A",
                  glowColor: "rgba(229, 164, 74, 0.5)",
                  bgColor: "rgba(229, 164, 74, 0.12)",
                  description: "당신",
                }}
                lambda={0}
                theta={engineState.theta}
                isPending={false}
                isHuman
              />
            </div>
          </div>

          {/* Divider */}
          <div className="h-px mb-6" style={{ background: "var(--border-color)" }} />

          {/* Section: 참가자 초대 */}
          {onInvite && (
            <div className="mb-6">
              <div className="flex items-center gap-2 mb-4">
                <Users size={14} style={{ color: "var(--accent-warm)" }} />
                <h2 className="text-[13px] font-bold uppercase tracking-wider text-[var(--text-secondary)]">
                  참가자 초대
                </h2>
              </div>
              <InvitePanel
                personaCount={engineState.participants.filter((p) => p.id !== "나").length}
                onInvite={onInvite}
              />
            </div>
          )}

          {/* Divider */}
          <div className="h-px mb-6" style={{ background: "var(--border-color)" }} />

          {/* Section: Global metrics - 흐름 + 냉각도 합친 카드 */}
          <div>
            <div className="flex items-center gap-2 mb-4">
              <Thermometer size={14} style={{ color: "var(--accent-warm)" }} />
              <h2 className="text-[13px] font-bold uppercase tracking-wider text-[var(--text-secondary)]">
                방 상태
              </h2>
            </div>

            {/* 활기 + 흐름 + 냉각도 통합 카드 */}
            <div className="p-3 rounded-xl" style={{ background: "var(--bg-base)" }}>
              {/* 활기 바 (맨 위) */}
              <div className="mb-3">
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-[11px] font-medium text-[var(--text-secondary)]">활기</span>
                  <span
                    className="text-[11px] font-semibold tabular-nums"
                    style={{ color: livelinessDesc.color, transition: "color 0.6s ease" }}
                  >
                    {livelinessDesc.text}
                  </span>
                </div>
                <div className="h-2 rounded-full overflow-hidden" style={{ background: "var(--gauge-bg)" }}>
                  <div
                    className="h-full rounded-full"
                    style={{
                      width: `${engineState.liveliness * 100}%`,
                      background:
                        engineState.liveliness > 0.7
                          ? "linear-gradient(90deg, #C47A1A, #E5A44A)"
                          : engineState.liveliness > 0.4
                          ? "linear-gradient(90deg, #A06830, #D9A05B)"
                          : engineState.liveliness > 0.15
                          ? "linear-gradient(90deg, #8070AA, #A89FCC)"
                          : "linear-gradient(90deg, #505060, #7070A0)",
                      transition: "width 0.9s cubic-bezier(0.25, 1, 0.5, 1), background 0.6s ease",
                    }}
                  />
                </div>
              </div>

              {/* 흐름 바 */}
              <div className="mb-3">
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-[11px] font-medium text-[var(--text-secondary)]">흐름</span>
                  <span
                    className="text-[11px] font-semibold tabular-nums"
                    style={{ color: flowDesc.color, transition: "color 0.6s ease" }}
                  >
                    {flowDesc.text}
                  </span>
                </div>
                <div className="h-2 rounded-full overflow-hidden" style={{ background: "var(--gauge-bg)" }}>
                  <div
                    className="h-full rounded-full"
                    style={{
                      width: `${engineState.flow * 100}%`,
                      background: `linear-gradient(90deg, #8ABF9F, ${flowDesc.color})`,
                      transition: "width 0.9s cubic-bezier(0.25, 1, 0.5, 1), background 0.6s ease",
                    }}
                  />
                </div>
              </div>

              {/* 냉각도 바 */}
              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <span className="text-[11px] font-medium text-[var(--text-secondary)]">냉각도</span>
                  <span
                    className="text-[11px] font-semibold tabular-nums"
                    style={{
                      color: muPct > 0.6 ? "#8ABF9F" : muPct > 0.35 ? "#E5A44A" : "#D9645A",
                      transition: "color 0.6s ease",
                    }}
                  >
                    {muDesc.text}
                  </span>
                </div>
                <div className="h-2 rounded-full overflow-hidden" style={{ background: "var(--gauge-bg)" }}>
                  <div
                    className="h-full rounded-full"
                    style={{
                      width: `${muPct * 100}%`,
                      background:
                        muPct > 0.6
                          ? "linear-gradient(90deg, #6DAF87, #8ABF9F)"
                          : muPct > 0.35
                          ? "linear-gradient(90deg, #C47A1A, #E5A44A)"
                          : "linear-gradient(90deg, #B03530, #D9645A)",
                      transition: "width 0.9s cubic-bezier(0.25, 1, 0.5, 1), background 0.6s ease",
                    }}
                  />
                </div>
              </div>
            </div>
          </div>

          {/* Section: 발화 속도 */}
          {onPace && (
            <>
              <div className="h-px my-6" style={{ background: "var(--border-color)" }} />
              <div>
                <div className="flex items-center gap-2 mb-4">
                  <Zap size={14} style={{ color: "var(--accent-warm)" }} />
                  <h2 className="text-[13px] font-bold uppercase tracking-wider text-[var(--text-secondary)]">
                    발화 속도
                  </h2>
                </div>
                <div className="grid grid-cols-2 gap-1.5">
                  {([
                    { label: "빠름", ms: 1500 },
                    { label: "보통", ms: 3000 },
                    { label: "느림", ms: 6000 },
                    { label: "매우 느림", ms: 12000 },
                  ] as { label: string; ms: number }[]).map(({ label, ms }) => {
                    const isActive = engineState.tick_ms === ms;
                    return (
                      <button
                        key={ms}
                        onClick={() => onPace(ms)}
                        className="px-3 py-2 rounded-lg text-[12px] font-medium transition-all duration-200"
                        style={{
                          background: isActive ? "rgba(229, 164, 74, 0.18)" : "var(--bg-base)",
                          color: isActive ? "var(--accent-warm)" : "var(--text-secondary)",
                          border: isActive
                            ? "1px solid rgba(229, 164, 74, 0.45)"
                            : "1px solid var(--border-color)",
                        }}
                      >
                        {label}
                      </button>
                    );
                  })}
                </div>
              </div>
            </>
          )}

          {/* 조용한 순간 (사이드바 하단) */}
          {isSilent && (
            <div
              className="mt-6 px-3 py-2 rounded-lg text-xs text-center"
              style={{
                background: "rgba(138, 191, 159, 0.08)",
                color: "#8ABF9F",
                border: "1px solid rgba(138, 191, 159, 0.15)",
              }}
            >
              <span className="inline-flex items-center gap-1.5">
                <Wind size={12} />
                조용한 순간 - 누군가 말하기를 기다리는 중
              </span>
            </div>
          )}
        </div>
      </aside>
    </>
  );
}
