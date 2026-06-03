import { useMemo } from "react";
import { Activity, Thermometer, Wind, X } from "lucide-react";
import { PersonaPresence } from "./PersonaPresence";
import type { EngineState, PersonaConfig } from "@/types";

interface SidePanelProps {
  engineState: EngineState;
  personaConfigs: PersonaConfig[];
  open: boolean;
  onClose: () => void;
  humanPulse?: boolean;
}

export function SidePanel({ engineState, personaConfigs, open, onClose, humanPulse = false }: SidePanelProps) {
  // Compute silence status
  const isSilent = useMemo(() => {
    const ids = Object.keys(engineState.intensities).filter((id) => id !== "나");
    return ids.every((id) => engineState.intensities[id] < engineState.theta);
  }, [engineState.intensities, engineState.theta]);

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
              {personaConfigs
                .filter((p) => p.id !== "나")
                .map((config) => {
                  const participant = engineState.participants.find((p) => p.id === config.id);
                  return (
                    <PersonaPresence
                      key={config.id}
                      config={config}
                      lambda={engineState.intensities[config.id] ?? 0}
                      theta={engineState.theta}
                      isPending={engineState.pending === config.id}
                      model={participant?.model}
                      humanPulse={humanPulse}
                    />
                  );
                })}

              {/* Human card */}
              <PersonaPresence
                config={personaConfigs.find((p) => p.id === "나")!}
                lambda={0}
                theta={engineState.theta}
                isPending={false}
                isHuman
              />
            </div>
          </div>

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

            {/* 흐름 + 냉각도 통합 카드 */}
            <div className="p-3 rounded-xl" style={{ background: "var(--bg-base)" }}>
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
