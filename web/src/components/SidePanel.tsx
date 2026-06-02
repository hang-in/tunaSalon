import { useMemo } from "react";
import { Activity, Thermometer, Wind, X } from "lucide-react";
import { PersonaPresence } from "./PersonaPresence";
import type { EngineState, PersonaConfig } from "@/types";

interface SidePanelProps {
  engineState: EngineState;
  personaConfigs: PersonaConfig[];
  open: boolean;
  onClose: () => void;
}

export function SidePanel({ engineState, personaConfigs, open, onClose }: SidePanelProps) {
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
    if (m > 0.85) return { text: "활기참", pct: 1.0 };
    if (m > 0.65) return { text: "적정", pct: 0.7 };
    if (m > 0.45) return { text: "냉각 중", pct: 0.4 };
    return { text: "조용함", pct: 0.15 };
  }, [engineState.mu_scale]);

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

            {/* Silence indicator */}
            {isSilent && (
              <div
                className="mb-3 px-3 py-2 rounded-lg text-xs text-center"
                style={{
                  background: "rgba(138, 191, 159, 0.08)",
                  color: "#8ABF9F",
                  border: "1px solid rgba(138, 191, 159, 0.15)",
                }}
              >
                <span className="inline-flex items-center gap-1.5">
                  <Wind size={12} />
                  조용한 순간 — 누군가 말하기를 기다리는 중
                </span>
              </div>
            )}

            <div className="flex flex-col gap-3">
              {personaConfigs
                .filter((p) => p.id !== "나")
                .map((config) => (
                  <PersonaPresence
                    key={config.id}
                    config={config}
                    lambda={engineState.intensities[config.id] ?? 0}
                    theta={engineState.theta}
                    isPending={engineState.pending === config.id}
                  />
                ))}

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

          {/* Section: Global metrics */}
          <div>
            <div className="flex items-center gap-2 mb-4">
              <Thermometer size={14} style={{ color: "var(--accent-warm)" }} />
              <h2 className="text-[13px] font-bold uppercase tracking-wider text-[var(--text-secondary)]">
                방 상태
              </h2>
            </div>

            {/* Flow gauge */}
            <div className="mb-4 p-3 rounded-xl" style={{ background: "var(--bg-base)" }}>
              <div className="flex items-center justify-between mb-2">
                <span className="text-[12px] font-medium text-[var(--text-secondary)]">
                  흐름 (flow)
                </span>
                <span className="text-[12px] font-bold tabular-nums" style={{ color: flowDesc.color }}>
                  {flowDesc.text}
                </span>
              </div>
              <div className="h-2 rounded-full overflow-hidden" style={{ background: "var(--gauge-bg)" }}>
                <div
                  className="h-full rounded-full transition-all duration-700"
                  style={{
                    width: `${engineState.flow * 100}%`,
                    background: `linear-gradient(90deg, #8ABF9F, ${flowDesc.color})`,
                  }}
                />
              </div>
              <div className="flex justify-between mt-1">
                <span className="text-[10px] text-[var(--text-secondary)]">활발</span>
                <span className="text-[10px] text-[var(--text-secondary)]">수렴</span>
              </div>
            </div>

            {/* mu_scale indicator */}
            <div className="p-3 rounded-xl" style={{ background: "var(--bg-base)" }}>
              <div className="flex items-center justify-between mb-2">
                <span className="text-[12px] font-medium text-[var(--text-secondary)]">
                  냉각도 (cool)
                </span>
                <span className="text-[12px] font-bold tabular-nums" style={{ color: "var(--text-secondary)" }}>
                  {muDesc.text}
                </span>
              </div>
              <div className="flex gap-1">
                {[1, 2, 3, 4, 5].map((i) => (
                  <div
                    key={i}
                    className="flex-1 h-6 rounded-md transition-all duration-500"
                    style={{
                      background:
                        i / 5 <= muDesc.pct
                          ? `rgba(229, 164, 74, ${0.2 + (i / 5) * 0.5})`
                          : "var(--gauge-bg)",
                      border:
                        i / 5 <= muDesc.pct
                          ? "1px solid rgba(229, 164, 74, 0.3)"
                          : "1px solid transparent",
                    }}
                  />
                ))}
              </div>
              <div className="flex justify-between mt-1">
                <span className="text-[10px] text-[var(--text-secondary)]">활발</span>
                <span className="text-[10px] text-[var(--text-secondary)]">차분</span>
              </div>
            </div>
          </div>
        </div>
      </aside>
    </>
  );
}
