// 절차적 SVG 캐릭터 아바타. 4축(혈액형·MBTI·별자리·역할) + λ 포즈에서 결정적으로 생성.
// 래스터 에셋 없음 — 같은 입력이면 같은 그림(precompute 불필요, on-demand 조립).
//
// 매핑(persona-ui §4):
//   역할   → 머리/소품 실루엣
//   혈액형 → 팔레트 주색(BLOOD_PALETTE, Rust palette_hex와 동일)
//   별자리 → 우하단 심볼 배지
//   MBTI   → E/I 볼터치, T/F 눈썹 각도
//   λ 포즈 → 표정/포즈(졸음→들썩→손번쩍→발화)

import { memo, type ReactElement } from "react";

export type AvatarPose = "sleep" | "calm" | "antsy" | "ready" | "speaking";

export interface AvatarAxes {
  blood: string;
  mbti: string;
  zodiac: string;
  role: string;
}

// Rust persona_kit Blood::palette_hex 와 동일.
const BLOOD_PALETTE: Record<string, string> = {
  A: "#6B8FD4", // 차분한 블루
  B: "#E0784A", // 활기찬 오렌지
  O: "#D44F4F", // 열정적인 레드
  AB: "#8C6BAE", // 신비로운 퍼플
};

const ZODIAC_SYMBOL: Record<string, string> = {
  ari: "♈", tau: "♉", gem: "♊", can: "♋", leo: "♌", vir: "♍",
  lib: "♎", sco: "♏", sag: "♐", cap: "♑", aqu: "♒", pis: "♓",
};

function clamp(n: number) {
  return Math.max(0, Math.min(255, Math.round(n)));
}

function toRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const v = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  return [parseInt(v.slice(0, 2), 16), parseInt(v.slice(2, 4), 16), parseInt(v.slice(4, 6), 16)];
}

/** hex를 target(흰/검) 쪽으로 t(0~1) 만큼 섞는다. */
function mix(hex: string, target: string, t: number): string {
  const [r1, g1, b1] = toRgb(hex);
  const [r2, g2, b2] = toRgb(target);
  const r = clamp(r1 + (r2 - r1) * t);
  const g = clamp(g1 + (g2 - g1) * t);
  const b = clamp(b1 + (b2 - b1) * t);
  return `#${[r, g, b].map((n) => n.toString(16).padStart(2, "0")).join("")}`;
}
const lighten = (hex: string, t: number) => mix(hex, "#ffffff", t);
const darken = (hex: string, t: number) => mix(hex, "#000000", t);

// 혈액형이 없는(구 방/사람 미설정) 참가자에 id 해시로 배정하는 색.
const HASH_PALETTE = ["#7BA7D9", "#D99A5B", "#9FCC8A", "#CC8AB8", "#5BC0BE"];

function hexToRgba(hex: string, a: number): string {
  const [r, g, b] = toRgb(hex);
  return `rgba(${r}, ${g}, ${b}, ${a})`;
}

/** 참가자 색 세트(아바타·게이지·카드 공용). 혈액형 팔레트 우선, 없으면 id 해시. */
export function personaColorSet(
  id: string,
  blood?: string,
): { color: string; glowColor: string; bgColor: string } {
  const b = blood?.toUpperCase();
  let base = (b && BLOOD_PALETTE[b]) || undefined;
  if (!base) {
    let h = 0;
    for (const ch of id) h = (h + ch.charCodeAt(0)) % HASH_PALETTE.length;
    base = HASH_PALETTE[h];
  }
  return { color: base, glowColor: hexToRgba(base, 0.5), bgColor: hexToRgba(base, 0.12) };
}

/** 역할별 머리/소품. 얼굴(이마 위)에 얹는다. */
function roleHair(role: string, hair: string, accent: string): ReactElement {
  switch (role) {
    case "chaos": // 반론자/와일드카드 — 삐죽 머리
      return (
        <path
          d="M7 13 L10 4 L13 11 L16 3 L20 10 L24 3 L27 11 L30 4 L33 13 Z"
          fill={hair}
        />
      );
    case "strategist": // 전략가 — 베레모
      return (
        <g>
          <path d="M7 12 Q20 1 33 12 Q20 7 7 12 Z" fill={hair} />
          <circle cx="31" cy="6" r="1.6" fill={accent} />
        </g>
      );
    case "summarizer": // 정리자 — 단정한 옆가르마
      return <path d="M6 13 Q20 -1 34 13 Q28 7 20 8 Q12 7 6 13 Z" fill={hair} />;
    case "critic": // 비평가 — 짧고 각진 머리
      return <path d="M7 12 L7 7 Q20 0 33 7 L33 12 Q20 6 7 12 Z" fill={hair} />;
    case "realist": // 현실주의자 — 차분한 둥근 머리(안경은 따로)
      return <path d="M7 13 Q20 2 33 13 Q20 8 7 13 Z" fill={hair} />;
    default: // 그 외 — 부드러운 돔
      return <path d="M7 13 Q20 0 33 13 Q20 6 7 13 Z" fill={hair} />;
  }
}

/** 포즈별 눈. */
function eyes(pose: AvatarPose, ink: string): ReactElement {
  if (pose === "sleep") {
    return (
      <g stroke={ink} strokeWidth="1.4" strokeLinecap="round" fill="none">
        <path d="M13 20 Q15.5 22 18 20" />
        <path d="M22 20 Q24.5 22 27 20" />
      </g>
    );
  }
  if (pose === "ready") {
    return (
      <g>
        <circle cx="15.5" cy="20" r="2.6" fill="#fff" stroke={ink} strokeWidth="0.6" />
        <circle cx="24.5" cy="20" r="2.6" fill="#fff" stroke={ink} strokeWidth="0.6" />
        <circle cx="15.8" cy="20.2" r="1.3" fill={ink} />
        <circle cx="24.8" cy="20.2" r="1.3" fill={ink} />
      </g>
    );
  }
  // antsy는 위를 보는 눈, calm/speaking은 정면 점눈
  const cy = pose === "antsy" ? 19.2 : 20;
  return (
    <g fill={ink}>
      <circle cx="15.5" cy={cy} r="1.7" />
      <circle cx="24.5" cy={cy} r="1.7" />
    </g>
  );
}

/** 포즈별 입. */
function mouth(pose: AvatarPose, ink: string): ReactElement {
  switch (pose) {
    case "sleep":
      return <path d="M18.5 27 L21.5 27" stroke={ink} strokeWidth="1.2" strokeLinecap="round" />;
    case "speaking":
      return <ellipse cx="20" cy="27.5" rx="2.6" ry="2" fill={ink} />;
    case "ready":
      return <circle cx="20" cy="27" r="1.8" fill={ink} />;
    case "antsy":
      return (
        <path d="M17 27 Q18.5 25.8 20 27 Q21.5 28.2 23 27" stroke={ink} strokeWidth="1.2" fill="none" strokeLinecap="round" />
      );
    default: // calm — 잔잔한 미소
      return <path d="M16.5 26 Q20 29.5 23.5 26" stroke={ink} strokeWidth="1.3" fill="none" strokeLinecap="round" />;
  }
}

/** MBTI T/F 눈썹(비평가는 더 날카롭게). */
function brows(role: string, mbti: string, ink: string): ReactElement | null {
  const isT = mbti.length >= 3 && mbti[2] === "T";
  if (role === "critic") {
    // 안쪽으로 처진 날카로운 눈썹(찌푸림)
    return (
      <g stroke={ink} strokeWidth="1.3" strokeLinecap="round">
        <path d="M13 16 L18 17.4" />
        <path d="M27 16 L22 17.4" />
      </g>
    );
  }
  if (isT) {
    // 평평한 눈썹
    return (
      <g stroke={ink} strokeWidth="1.1" strokeLinecap="round">
        <path d="M13.5 16.5 L17.5 16.5" />
        <path d="M22.5 16.5 L26.5 16.5" />
      </g>
    );
  }
  // F — 살짝 올라간 부드러운 눈썹
  return (
    <g stroke={ink} strokeWidth="1.1" strokeLinecap="round" fill="none">
      <path d="M13.5 16.8 Q15.5 15.8 17.5 16.4" />
      <path d="M22.5 16.4 Q24.5 15.8 26.5 16.8" />
    </g>
  );
}

export const PersonaAvatar = memo(function PersonaAvatar({
  axes,
  color,
  pose,
  size = 36,
}: {
  axes?: AvatarAxes;
  color: string;
  pose: AvatarPose;
  size?: number;
}) {
  const blood = axes?.blood?.toUpperCase() ?? "";
  const base = BLOOD_PALETTE[blood] ?? color;
  const face = lighten(base, 0.5);
  const ink = darken(base, 0.45);
  const hair = darken(base, 0.2);
  const role = axes?.role ?? "";
  const mbti = (axes?.mbti ?? "").toUpperCase();
  const isExtrovert = mbti.startsWith("E");
  const zodiac = axes?.zodiac ?? "";
  const symbol = ZODIAC_SYMBOL[zodiac];

  return (
    <svg width={size} height={size} viewBox="0 0 40 40" aria-hidden="true">
      {/* 얼굴 */}
      <ellipse cx="20" cy="20" rx="13" ry="13.5" fill={face} stroke={darken(base, 0.1)} strokeWidth="0.8" />

      {/* E형 볼터치 */}
      {isExtrovert && (
        <g fill={base} opacity="0.45">
          <ellipse cx="12.5" cy="24" rx="2.2" ry="1.4" />
          <ellipse cx="27.5" cy="24" rx="2.2" ry="1.4" />
        </g>
      )}

      {brows(role, mbti, ink)}
      {eyes(pose, ink)}
      {mouth(pose, ink)}

      {/* 현실주의자 안경 */}
      {role === "realist" && (
        <g stroke={ink} strokeWidth="0.9" fill="none">
          <circle cx="15.5" cy="20" r="3.4" />
          <circle cx="24.5" cy="20" r="3.4" />
          <path d="M18.9 20 L21.1 20" />
        </g>
      )}

      {/* 머리/소품 (이마 위) */}
      {roleHair(role, hair, lighten(base, 0.6))}

      {/* 포즈 부가 요소 */}
      {pose === "ready" && (
        // 번쩍 든 손
        <g stroke={base} strokeWidth="1.6" strokeLinecap="round" fill={face}>
          <path d="M31 17 L34 9" />
          <circle cx="34.5" cy="7.5" r="2.2" fill={lighten(base, 0.5)} stroke={darken(base, 0.1)} strokeWidth="0.6" />
        </g>
      )}
      {pose === "speaking" && (
        // 말하는 중 모션 호
        <g stroke={base} strokeWidth="1.1" fill="none" strokeLinecap="round" opacity="0.8">
          <path d="M34 24 Q36 26 34 28" />
          <path d="M36.5 22 Q39.5 26 36.5 30" opacity="0.5" />
        </g>
      )}
      {pose === "sleep" && (
        <text x="30" y="11" fontSize="7" fill={ink} opacity="0.6" fontWeight="700">z</text>
      )}
      {pose === "antsy" && (
        // 땀방울
        <path d="M31 16 Q33 19 31 20.5 Q29.5 19 31 16 Z" fill="#6FB7E0" opacity="0.85" />
      )}

      {/* 별자리 심볼 배지(우하단) */}
      {symbol && (
        <g>
          <circle cx="31.5" cy="31.5" r="6" fill={base} stroke="#fff" strokeWidth="1" />
          <text
            x="31.5"
            y="34.3"
            fontSize="7.5"
            textAnchor="middle"
            fill="#fff"
            fontWeight="700"
          >
            {symbol}
          </text>
        </g>
      )}
    </svg>
  );
});

/** λ 밴드 → 포즈. PersonaPresence의 기존 밴드 기준과 동일. */
export function poseFromLambda(
  lambda: number,
  theta: number,
  isPending: boolean,
): AvatarPose {
  const pct = Math.max(0, Math.min(1, lambda));
  if (isPending) return "speaking";
  if (pct >= theta) return "ready";
  if (pct >= theta * 0.7) return "antsy";
  if (pct < theta * 0.3) return "sleep";
  return "calm";
}
