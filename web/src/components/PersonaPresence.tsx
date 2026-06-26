import { memo } from "react";
import type { PersonaConfig, Participant } from "@/types";
import { PersonaAvatar, poseFromLambda } from "@/lib/personaAvatar";

interface PersonaPresenceProps {
  config: PersonaConfig;
  lambda: number;
  theta: number;
  isPending: boolean;
  isHuman?: boolean;
  model?: string;
  humanPulse?: boolean;
  axes?: Participant["axes"];
}

function PersonaPresenceRaw({ config, lambda, theta, isPending, isHuman = false, model, humanPulse = false, axes }: PersonaPresenceProps) {
  const pct = Math.max(0, Math.min(1, lambda));

  const pose = poseFromLambda(lambda, theta, isPending);

  // λ-band 분류
  const isActive = isPending || pct >= theta;
  const isAntsy = !isActive && pct >= theta * 0.7; // 들썩임 구간

  // glow intensity
  const glowShadow = humanPulse
    ? `0 0 18px ${config.glowColor}`
    : isPending
    ? `0 0 12px ${config.glowColor}`
    : isActive
    ? `0 0 6px ${config.glowColor}`
    : "none";

  return (
    <div
      className={`persona-card rounded-xl p-2.5 ${humanPulse && !isHuman ? "human-pulse-card" : ""} ${isAntsy ? "antsy-card" : ""}`}
      style={{
        background: config.bgColor,
        borderWidth: 1,
        borderStyle: "solid",
        borderColor: humanPulse && !isHuman
          ? config.color
          : isPending
          ? config.color
          : pct >= theta
          ? `${config.color}55`
          : "transparent",
        boxShadow: humanPulse && !isHuman
          ? `0 0 16px ${config.glowColor}`
          : isPending
          ? `0 0 10px ${config.glowColor}`
          : pct >= theta
          ? `0 0 5px ${config.glowColor}30`
          : "none",
      }}
    >
      <div className="flex items-center gap-2.5">
        {/* Avatar (단순 원; λ 게이지는 카드 하단 바로 표시) */}
        <div
          className={`rounded-full shrink-0 flex items-center justify-center ${humanPulse && !isHuman ? "human-pulse-ring" : ""}`}
          style={{
            width: 36,
            height: 36,
            background: config.bgColor,
            color: config.color,
            fontSize: 13,
            fontWeight: 700,
            border: `1.5px solid ${isActive ? config.color : `${config.color}33`}`,
            boxShadow: glowShadow,
            transition: "transform 0.3s ease, box-shadow 0.35s ease, border-color 0.35s ease",
            transform: isPending ? "scale(1.08)" : pct >= theta ? "scale(1.04)" : "scale(1)",
          }}
        >
          {isHuman ? (
            <span style={{ fontSize: 14 }}>🧑</span>
          ) : (
            <PersonaAvatar axes={axes} color={config.color} pose={pose} size={34} />
          )}
        </div>

        {/* Name + model + pending dots */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-1.5 flex-wrap">
            <span className="text-[12px] font-semibold text-[var(--text-primary)] truncate">
              {config.name}
            </span>
            {isPending && (
              <span
                className="inline-flex items-center gap-0.5"
                aria-label="생각하는 중"
                title="생각하는 중"
              >
                {[0, 1, 2].map((i) => (
                  <span
                    key={i}
                    className="w-1 h-1 rounded-full typing-dot"
                    style={{ background: config.color }}
                  />
                ))}
              </span>
            )}
          </div>
          {!isHuman && model && (
            <span className="text-[10px] text-[var(--text-secondary)] truncate block leading-tight">
              {model}
            </span>
          )}
          {isHuman && (
            <span className="text-[10px] text-[var(--text-secondary)] truncate block leading-tight">
              {config.description}
            </span>
          )}
        </div>
      </div>

      {/* λ 게이지: 카드 하단 얇은 바(사람 제외). θ 마커, 수치 없음. */}
      {!isHuman && (
        <div className="mt-2 relative h-1 rounded-full overflow-hidden" style={{ background: "var(--gauge-bg)" }}>
          <div
            className="absolute top-0 bottom-0 z-10"
            style={{
              left: `${Math.min(1, Math.max(0, theta)) * 100}%`,
              width: 1,
              background: "rgba(255,255,255,0.45)",
            }}
          />
          <div
            className="h-full rounded-full"
            style={{
              width: `${pct * 100}%`,
              background: isActive ? config.color : `${config.color}aa`,
              transition: "width 0.5s cubic-bezier(0.25,1,0.5,1), background 0.35s ease",
            }}
          />
        </div>
      )}
    </div>
  );
}

export const PersonaPresence = memo(PersonaPresenceRaw);
