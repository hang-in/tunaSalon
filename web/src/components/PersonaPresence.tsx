import { memo } from "react";
import { LambdaGauge } from "./LambdaGauge";
import type { PersonaConfig } from "@/types";

interface PersonaPresenceProps {
  config: PersonaConfig;
  lambda: number;
  theta: number;
  isPending: boolean;
  isHuman?: boolean;
}

function PersonaPresenceRaw({ config, lambda, theta, isPending, isHuman = false }: PersonaPresenceProps) {
  const pct = Math.max(0, Math.min(1, lambda));

  // Expression state based on λ band
  const getExpression = () => {
    if (isPending) return "speaking";
    if (pct >= theta) return "hand-up";
    if (pct >= theta * 0.7) return "fidgety";
    if (pct < theta * 0.3) return "idle";
    return "listening";
  };

  const expr = getExpression();

  // Avatar silhouette: simple emoji-based expressions with CSS
  const getAvatarContent = () => {
    if (isHuman) return "나";
    switch (expr) {
      case "idle": return "◡";
      case "listening": return "‿";
      case "fidgety": return "•̀";
      case "hand-up": return "☝";
      case "speaking": return "◠";
      default: return "◡";
    }
  };

  const getGlowIntensity = () => {
    if (isPending) return 1;
    if (pct >= theta) return 0.8;
    if (pct >= theta * 0.7) return 0.5;
    return 0.2;
  };

  return (
    <div
      className="persona-card rounded-xl p-3"
      style={{
        background: config.bgColor,
        borderWidth: 1,
        borderStyle: "solid",
        borderColor: isPending
          ? config.color
          : pct >= theta
            ? `${config.color}66`
            : "transparent",
        boxShadow: isPending
          ? `0 0 12px ${config.glowColor}`
          : pct >= theta
            ? `0 0 6px ${config.glowColor}40`
            : "none",
      }}
    >
      <div className="flex items-center gap-3 mb-2.5">
        {/* Avatar */}
        <div
          className={`
            w-9 h-9 rounded-full flex items-center justify-center
            text-sm font-bold transition-all duration-300
            ${isPending ? "scale-110" : pct >= theta ? "scale-105" : "scale-100"}
          `}
          style={{
            background: `${config.color}22`,
            color: config.color,
            boxShadow: `0 0 ${getGlowIntensity() * 16}px ${config.glowColor}`,
          }}
        >
          {isHuman ? (
            <span className="text-xs">🧑</span>
          ) : (
            <span className="text-xs select-none">{getAvatarContent()}</span>
          )}
        </div>

        {/* Name + status */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-[13px] font-semibold text-[var(--text-primary)] truncate">
              {config.name}
            </span>
            {isPending && (
              <span
                className="inline-flex items-center gap-1 px-2 py-1 rounded-full"
                style={{ background: config.color }}
                aria-label="생각하는 중"
                title="생각하는 중"
              >
                {[0, 1, 2].map((i) => (
                  <span
                    key={i}
                    className="w-1.5 h-1.5 rounded-full typing-dot"
                    style={{ background: "#fff" }}
                  />
                ))}
              </span>
            )}
          </div>
          <span className="text-[11px] text-[var(--text-secondary)]">
            {isHuman ? config.description : exprLabel(expr)}
          </span>
        </div>
      </div>

      {/* Lambda gauge (skip for human) */}
      {!isHuman && (
        <LambdaGauge
          value={lambda}
          theta={theta}
          isPending={isPending}
          config={config}
          label={config.id.slice(0, 3)}
        />
      )}
    </div>
  );
}

function exprLabel(expr: string): string {
  switch (expr) {
    case "idle": return "조용히 듣는 중";
    case "listening": return "관심 있게 듣는 중";
    case "fidgety": return "말하고 싶어 안달";
    case "hand-up": return "거의 말하려 함";
    case "speaking": return "생각을 말하는 중";
    default: return "";
  }
}

export const PersonaPresence = memo(PersonaPresenceRaw);
