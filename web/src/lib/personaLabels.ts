// 페르소나 4축 raw 토큰 -> 표시 문자열 매핑.
// InvitePanel의 ZODIAC_OPTIONS/ROLE_OPTIONS와 동일 데이터를 공용 모듈로 추출.

export const ZODIAC_LABEL: Record<string, string> = {
  ari: "양자리",
  tau: "황소자리",
  gem: "쌍둥이자리",
  can: "게자리",
  leo: "사자자리",
  vir: "처녀자리",
  lib: "천칭자리",
  sco: "전갈자리",
  sag: "사수자리",
  cap: "염소자리",
  aqu: "물병자리",
  pis: "물고기자리",
};

export const ROLE_LABEL: Record<string, string> = {
  friend: "친구",
  chaos: "와일드카드",
  critic: "비평가",
  realist: "현실주의자",
  teacher: "교사",
  poet: "시인",
  strategist: "전략가",
  summarizer: "정리자",
};

/** "O" -> "O형" */
export function bloodLabel(blood: string): string {
  return `${blood}형`;
}

/** zodiac raw 토큰 -> 한글 (없으면 raw 그대로) */
export function zodiacLabel(zodiac: string): string {
  return ZODIAC_LABEL[zodiac] ?? zodiac;
}

/** role raw 토큰 -> 한글 (없으면 raw 그대로) */
export function roleLabel(role: string): string {
  return ROLE_LABEL[role] ?? role;
}
