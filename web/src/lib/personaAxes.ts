// 참가자 축(혈액형·MBTI·별자리·역할) 선택지. 데이터 전용(컴포넌트는 AxisSelect.tsx).
// InvitePanel(방 안 초대)과 CreateRoomDialog(새 방 빌더)가 공유한다.

export interface AxisOption {
  value: string;
  label: string;
}

export const MAX_PERSONAS = 3;

export const BLOOD_OPTIONS: AxisOption[] = [
  { value: "A", label: "A형" },
  { value: "B", label: "B형" },
  { value: "O", label: "O형" },
  { value: "AB", label: "AB형" },
];

export const MBTI_OPTIONS: AxisOption[] = [
  "ENTP", "ENTJ", "ENFP", "ENFJ",
  "ESTP", "ESTJ", "ESFP", "ESFJ",
  "INTP", "INTJ", "INFP", "INFJ",
  "ISTP", "ISTJ", "ISFP", "ISFJ",
].map((m) => ({ value: m, label: m }));

export const ZODIAC_OPTIONS: AxisOption[] = [
  { value: "ari", label: "양자리" },
  { value: "tau", label: "황소자리" },
  { value: "gem", label: "쌍둥이자리" },
  { value: "can", label: "게자리" },
  { value: "leo", label: "사자자리" },
  { value: "vir", label: "처녀자리" },
  { value: "lib", label: "천칭자리" },
  { value: "sco", label: "전갈자리" },
  { value: "sag", label: "사수자리" },
  { value: "cap", label: "염소자리" },
  { value: "aqu", label: "물병자리" },
  { value: "pis", label: "물고기자리" },
];

export const ROLE_OPTIONS: AxisOption[] = [
  { value: "friend", label: "친구" },
  { value: "chaos", label: "와일드카드" },
  { value: "critic", label: "비평가" },
  { value: "realist", label: "현실주의자" },
  { value: "teacher", label: "교사" },
  { value: "poet", label: "시인" },
  { value: "strategist", label: "전략가" },
  { value: "summarizer", label: "정리자" },
];
