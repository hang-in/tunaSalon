import { memo } from "react";
import type { PersonaConfig } from "@/types";

interface LambdaGaugeProps {
  value: number;     // λ 0..1
  theta: number;     // θ threshold
  isPending: boolean;
  config: PersonaConfig;
  label: string;
}

function LambdaGaugeRaw({ value, theta, isPending, config, label }: LambdaGaugeProps) {
  const pct = Math.max(0, Math.min(1, value));
  const thetaPct = Math.max(0, Math.min(1, theta));

  // λ band determines visual state
  const isAntsy = pct >= thetaPct * 0.7 && pct < thetaPct;
  const isOverThreshold = pct >= thetaPct;
  const isSpeaking = isPending;

  return (
    <div className="w-full select-none">
      {/* Label row */}
      <div className="flex items-center justify-between mb-1.5">
        <span className="text-[11px] font-semibold tracking-wide uppercase" style={{ color: config.color }}>
          λ {label}
        </span>
        <span
          className="text-[11px] tabular-nums font-medium"
          style={{ color: isOverThreshold ? config.color : "var(--text-secondary)" }}
        >
          {pct.toFixed(2)}
        </span>
      </div>

      {/* Gauge track */}
      <div className="relative h-2 rounded-full overflow-hidden" style={{ background: "var(--gauge-bg)" }}>
        {/* Theta marker line */}
        <div
          className="absolute top-0 bottom-0 w-px z-10"
          style={{
            left: `${thetaPct * 100}%`,
            background: "rgba(255,255,255,0.5)",
          }}
        />
        <div
          className="absolute z-10"
          style={{
            left: `${thetaPct * 100}%`,
            top: "-3px",
            transform: "translateX(-50%)",
          }}
        >
          <span className="text-[9px] font-bold text-white/50">θ</span>
        </div>

        {/* Fill bar */}
        <div
          className="h-full rounded-full relative overflow-hidden"
          style={{
            width: `${pct * 100}%`,
            background: isSpeaking
              ? config.color
              : isOverThreshold
                ? `linear-gradient(90deg, ${config.color}dd, ${config.color})`
                : `linear-gradient(90deg, ${config.color}88, ${config.color})`,
            boxShadow: isAntsy || isOverThreshold
              ? `0 0 8px ${config.glowColor}, 0 0 16px ${config.glowColor}`
              : "none",
            transition: "width 0.35s cubic-bezier(0.25, 1, 0.5, 1), box-shadow 0.35s ease, background 0.35s ease",
          }}
        >
          {isSpeaking && <div className="absolute inset-0 gauge-shimmer" />}
        </div>
      </div>
    </div>
  );
}

export const LambdaGauge = memo(LambdaGaugeRaw);
