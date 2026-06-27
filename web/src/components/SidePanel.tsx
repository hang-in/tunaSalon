import { useMemo, useState } from "react";
import { Activity, FileText, Thermometer, Wind, X, UserMinus, Users, Zap } from "lucide-react";
import { PersonaPresence } from "./PersonaPresence";
import { InvitePanel } from "./InvitePanel";
import { ReportMarkdown } from "@/components/ReportMarkdown";
import type { EngineState, PersonaConfig, ReportDto } from "@/types";

interface SidePanelProps {
  engineState: EngineState;
  personaConfigs: PersonaConfig[];
  getPersonaConfig: (id: string, blood?: string) => PersonaConfig;
  open: boolean;
  onClose: () => void;
  humanPulse?: boolean;
  onInvite?: (blood: string, mbti: string, zodiac: string, role?: string) => void;
  onRemove?: (id: string) => void;
  onPace?: (intervalMs: number) => void;
  onEditHuman?: () => void;
  reports?: ReportDto[];
}

export function SidePanel({ engineState, personaConfigs, getPersonaConfig, open, onClose, humanPulse = false, onInvite, onRemove, onPace, onEditHuman, reports = [] }: SidePanelProps) {
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

  const [modalReport, setModalReport] = useState<ReportDto | null>(null);

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
                  // PersonaConfig: 혈액형 팔레트(없으면 id 해시)로 색 통일 — 게이지·아바타 일치.
                  const config = getPersonaConfig(participant.id, participant.axes?.blood);
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

              {/* Human card — 클릭하면 내 캐릭터(4축) 구성. 이름은 서버가 보낸 캐릭터명(없으면 "나"). */}
              {(() => {
                const human = engineState.participants.find((p) => p.id === "나");
                const base = personaConfigs.find((p) => p.id === "나");
                const humanConfig = {
                  id: "나",
                  name: human?.name ?? "나",
                  color: base?.color ?? "#E5A44A",
                  glowColor: base?.glowColor ?? "rgba(229, 164, 74, 0.5)",
                  bgColor: base?.bgColor ?? "rgba(229, 164, 74, 0.12)",
                  description: onEditHuman ? "클릭해서 내 캐릭터 만들기" : "당신",
                };
                return (
                  <button
                    type="button"
                    onClick={onEditHuman}
                    disabled={!onEditHuman}
                    className="w-full text-left rounded-xl transition-opacity hover:opacity-90 disabled:cursor-default"
                    title={onEditHuman ? "내 캐릭터 만들기" : undefined}
                  >
                    <PersonaPresence
                      config={humanConfig}
                      lambda={0}
                      theta={engineState.theta}
                      isPending={false}
                      isHuman
                      axes={human?.axes}
                    />
                  </button>
                );
              })()}
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

          {/* Section: 지난 리포트 */}
          {reports.length > 0 && (
            <>
              <div className="h-px my-6" style={{ background: "var(--border-color)" }} />
              <div>
                <div className="flex items-center gap-2 mb-3">
                  <FileText size={14} style={{ color: "var(--accent-warm)" }} />
                  <h2 className="text-[13px] font-bold uppercase tracking-wider text-[var(--text-secondary)]">
                    지난 리포트
                  </h2>
                </div>
                <div className="flex flex-col gap-1.5">
                  {reports.map((r) => (
                    <button
                      key={r.seq}
                      onClick={() => setModalReport(r)}
                      className="w-full text-left px-3 py-2 rounded-lg text-[12px] transition-colors hover:opacity-80"
                      style={{
                        background: "var(--bg-base)",
                        border: "1px solid var(--border-color)",
                        color: "var(--text-secondary)",
                      }}
                    >
                      <span className="font-semibold" style={{ color: "var(--accent-warm)" }}>
                        #{r.seq}
                      </span>{" "}
                      <span className="truncate">{r.topic || r.conclusion.slice(0, 60)}</span>
                    </button>
                  ))}
                </div>
              </div>
            </>
          )}
        </div>
      </aside>

      {/* 리포트 전체 팝업 */}
      {modalReport && (
        <div
          className="fixed inset-0 z-[100] flex items-center justify-center p-4 bg-black/60"
          onClick={() => setModalReport(null)}
        >
          <div
            className="relative w-full max-w-2xl max-h-[80vh] overflow-y-auto rounded-2xl p-6"
            style={{ background: "var(--bg-surface)", border: "1px solid var(--border-color)" }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between mb-4">
              <div className="flex items-center gap-2">
                <FileText size={16} style={{ color: "var(--accent-warm)" }} />
                <span className="text-[14px] font-bold" style={{ color: "var(--accent-warm)" }}>
                  토론 리포트 #{modalReport.seq}
                </span>
                {modalReport.topic && (
                  <span className="text-[12px] text-[var(--text-secondary)]">· {modalReport.topic}</span>
                )}
              </div>
              <button
                onClick={() => setModalReport(null)}
                className="p-1.5 rounded-lg hover:bg-white/5 transition-colors"
                style={{ color: "var(--text-secondary)" }}
              >
                <X size={16} />
              </button>
            </div>
            <div className="text-[13.5px]" style={{ color: "var(--text-primary)" }}>
              <ReportMarkdown content={modalReport.markdown} />
            </div>
          </div>
        </div>
      )}
    </>
  );
}
