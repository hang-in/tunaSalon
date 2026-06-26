// 참가자 축(혈액형·MBTI·별자리·역할) 선택지. 데이터 전용(컴포넌트는 AxisSelect.tsx).
// InvitePanel(방 안 초대)과 CreateRoomDialog(새 방 빌더)가 공유한다.

export interface AxisOption {
  value: string;
  label: string;
  /** 선택지 위에 마우스를 올리면 뜨는 설명(역할 토론 성격 등). */
  hint?: string;
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

// 토론용 역할 5종(잡담형 친구·시인·해설자는 제외). hint는 선택 시 툴팁으로 표시된다.
export const ROLE_OPTIONS: AxisOption[] = [
  { value: "critic", label: "비평가", hint: "약한 전제와 과신을 날카롭게 반박합니다." },
  { value: "realist", label: "현실주의자", hint: "실현 가능성·비용·근거를 따집니다." },
  { value: "chaos", label: "반론자", hint: "떠오르는 합의에 반론하고 악마의 변호인을 맡습니다." },
  { value: "strategist", label: "전략가", hint: "쟁점을 정리해 결정해야 할 지점을 제시합니다." },
  { value: "summarizer", label: "정리자", hint: "양측의 핵심을 종합하고 결론을 압박합니다." },
];
