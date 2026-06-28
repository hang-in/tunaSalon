// 디버깅용: 페르소나 4축에서 한글 성격 설명을 만든다(닉네임 롤오버 툴팁).
// 역할은 잠정 폐기라 제외 — 개성은 MBTI/혈액형/별자리만.
// 백엔드 주입 프롬프트(영문)의 요약 한글판.

import { bloodLabel, zodiacLabel } from "@/lib/personaLabels";

const MBTI_KO: Record<string, string> = {
  ENTP: "재빠른 도발가 — 허점을 찌르고 더 엉뚱한 대안을 던짐, 새 질문 열기 선호",
  ENTJ: "결단형 지휘자 — 큰 그림·전략으로 결론을 밀어붙임",
  ENFP: "열정적 스파크 — 아이디어와 사람을 잇고 탄젠트로 튐",
  ENFJ: "카리스마 조율자 — 분위기 읽고 공통점으로 이끎",
  ESTP: "대담한 현실가 — 구체 사례로 이론을 뚫고 빠르게 움직임",
  ESTJ: "직설 조직가 — 사실·규칙·명확한 답을 원함",
  ESFP: "발랄한 현재형 — 생생하고 개인적이고 재미 위주",
  ESFJ: "따뜻한 연결자 — 분위기·포용 중시, 합의 지향",
  INTP: "정밀한 회의론자 — 정의를 해부하고 논리 일관성 추구, 질문 열어둠",
  INTJ: "전략적 장기 사고가 — 시스템과 흐름을 보고 깔끔한 결론 선호",
  INFP: "조용한 이상주의자 — 가치·의미 중시, 인간·윤리 측면에 끌림",
  INFJ: "통찰형 미래 사고가 — 패턴·의미를 감지, 부드럽게 결론으로",
  ISTP: "쿨한 실전 해결사 — 작동 원리에 집중, 사실·간결",
  ISTJ: "성실한 현실가 — 구체 디테일·검증된 사실, 깔끔히 마무리",
  ISFP: "잔잔한 감성가 — 개인적·미적 결을 느끼고 결론 강요 안 함",
  ISFJ: "배려심 디테일러 — 구체적 보살핌, 부드러운 마무리",
};

const BLOOD_KO: Record<string, string> = {
  A: "신중·세심 — 디테일과 타인 감정을 살핌, 다소 조심스러움",
  B: "자유분방·독립 — 자기 호기심대로, 솔직하고 굽히지 않음",
  O: "따뜻·열정 — 빠지면 올인, 사교적이나 승부욕·고집",
  AB: "쿨한 이중성 — 분석적이다가 장난기/강렬함, 독특한 시각",
};

const ZODIAC_KO: Record<string, string> = {
  ari: "대담·성급 — 행동 먼저, 직설·승부욕",
  tau: "느긋·뚝심 — 편안함 중시, 잘 안 움직임",
  gem: "재기발랄 — 아이디어 넘나듦, 위트·탄젠트",
  can: "따뜻·직관 — 감정에 민감, 주변을 챙김",
  leo: "표현·스포트라이트 — 자신감, 분위기 띄움, 인정 욕구",
  vir: "정밀·관찰 — 결함을 잡아냄, 까다롭지만 진심",
  lib: "균형·조화 — 양쪽 저울질, 중재자, 우유부단",
  sco: "강렬·집요 — 숨은 동기 감지, 깊이·대립 불사",
  sag: "낙천·모험 — 큰 그림, 직설·솔직, 산만할 수 있음",
  cap: "건조·절제 — 핵심만, 실리·야망, 차가워 보임",
  aqu: "독창·아웃사이더 — 남다른 각도, 관습보다 아이디어",
  pis: "몽환·공감 — 분위기 흡수, 감성·은유, 경계 흐림",
};

export interface DescAxes {
  blood: string;
  mbti: string;
  zodiac: string;
  role: string;
}

/** 닉네임 툴팁용 한글 설명. axes 없으면 빈 문자열(툴팁 미표시). */
export function personaDescription(axes?: DescAxes): string {
  if (!axes) return "";
  const mbti = (axes.mbti ?? "").toUpperCase();
  const blood = (axes.blood ?? "").toUpperCase();
  const zodiac = (axes.zodiac ?? "").toLowerCase();
  const head = `${mbti} · ${bloodLabel(blood)} · ${zodiacLabel(zodiac)}`;
  const lines = [MBTI_KO[mbti], BLOOD_KO[blood], ZODIAC_KO[zodiac]].filter(Boolean);
  return lines.length ? `${head}\n• ${lines.join("\n• ")}` : head;
}

/** 한 줄 성향 요약(MBTI 기준 대표 성향). axes 없으면 빈 문자열. */
export function personaTagline(axes?: DescAxes): string {
  if (!axes) return "";
  return MBTI_KO[(axes.mbti ?? "").toUpperCase()] ?? "";
}
