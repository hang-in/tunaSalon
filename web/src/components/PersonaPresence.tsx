import { memo } from "react";
import type { PersonaConfig } from "@/types";

interface PersonaPresenceProps {
  config: PersonaConfig;
  lambda: number;
  theta: number;
  isPending: boolean;
  isHuman?: boolean;
  model?: string;
  humanPulse?: boolean;
}

function PersonaPresenceRaw({ config, lambda, theta, isPending, isHuman = false, model, humanPulse = false }: PersonaPresenceProps) {
  const pct = Math.max(0, Math.min(1, lambda));

  // Avatar glyph expression based on λ band
  const getAvatarContent = () => {
    if (isHuman) return "나";
    if (isPending) return "◠";
    if (pct >= theta) return "☝";
    if (pct >= theta * 0.7) return "•̀";
    if (pct < theta * 0.3) return "◡";
    return "‿";
  };

  // λ-band 분류
  const isActive = isPending || pct >= theta;
  const isAntsy = !isActive && pct >= theta * 0.7; // 들썩임 구간

  // Ring color: above theta or pending = vivid, below = dim
  const ringColor = isActive ? config.color : `${config.color}55`;

  // glow intensity
  const glowShadow = humanPulse
    ? `0 0 18px ${config.glowColor}`
    : isPending
    ? `0 0 12px ${config.glowColor}`
    : isActive
    ? `0 0 6px ${config.glowColor}`
    : "none";

  // conic-gradient: 6시(from 180deg) 시계방향, pct * 360deg
  const ringDeg = pct * 360;
  const ringBg = isHuman
    ? "transparent"
    : `conic-gradient(from 180deg, ${ringColor} ${ringDeg}deg, var(--gauge-bg) ${ringDeg}deg)`;

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
        {/* Avatar with conic ring wrapper */}
        <div
          className={`rounded-full shrink-0 ${humanPulse && !isHuman ? "human-pulse-ring" : ""}`}
          style={{
            padding: isHuman ? 2 : 3,
            background: ringBg,
            boxShadow: glowShadow,
            transition: "background 0.4s cubic-bezier(0.25,1,0.5,1), box-shadow 0.35s ease",
          }}
        >
          <div
            className="rounded-full flex items-center justify-center"
            style={{
              width: 34,
              height: 34,
              // 불투명 배경: conic 진행률이 외곽 링(padding)에만 보이게(안쪽 비침 방지 -> 부채꼴 X).
              background: "var(--bg-surface)",
              color: config.color,
              fontSize: 13,
              fontWeight: 700,
              transition: "transform 0.3s ease",
              transform: isPending ? "scale(1.08)" : pct >= theta ? "scale(1.04)" : "scale(1)",
            }}
          >
            {isHuman ? (
              <span style={{ fontSize: 14 }}>🧑</span>
            ) : (
              <span className="select-none" style={{ lineHeight: 1 }}>{getAvatarContent()}</span>
            )}
          </div>
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
    </div>
  );
}

export const PersonaPresence = memo(PersonaPresenceRaw);
