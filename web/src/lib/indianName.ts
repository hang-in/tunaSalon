// 미리보기 전용 근사. SSOT는 Rust persona_kit::indian_name. 규칙 변경 시 동기화 필요.

/** 마지막 글자에 받침(종성)이 있는가(한글 음절 기준). */
function hasBatchim(s: string): boolean {
  const last = s[s.length - 1];
  if (!last) return false;
  const code = last.charCodeAt(0);
  if (code < 0xac00 || code > 0xd7a3) return false;
  return (code - 0xac00) % 28 !== 0;
}

/** 혈액형 -> 형용사. Blood::code 기준(A/B/O/AB). */
const BLOOD_ADJ: Record<string, string> = {
  A: "조용한",
  B: "지혜로운",
  O: "평화로운",
  AB: "날카로운",
};

/** MBTI -> 자연/동물 명사. Mbti::code 기준(4글자 대문자). */
const MBTI_NOUN: Record<string, string> = {
  ENTP: "늑대",
  ENTJ: "태양",
  ENFP: "바람",
  ENFJ: "강",
  ESTP: "불꽃",
  ESTJ: "황소",
  ESFP: "나비",
  ESFJ: "하늘",
  INTP: "여우",
  INTJ: "매",
  INFP: "안개",
  INFJ: "달",
  ISTP: "곰",
  ISTJ: "산",
  ISFP: "사슴",
  ISFJ: "별",
};

/**
 * 별자리 어미 생성.
 * Zodiac::abbreviation 기준(3글자 소문자).
 * 받침 있는 명사: 과/을, 없는 명사: 와/를.
 */
function zodiacSuffix(abbr: string, noun: string): string {
  const bat = hasBatchim(noun);
  const wa = bat ? "과" : "와";
  const eul = bat ? "을" : "를";
  switch (abbr) {
    case "ari": return "의 기상";
    case "tau": return "처럼 우직한";
    case "gem": return `${wa} 함께 춤을`;
    case "can": return "아래에서";
    case "leo": return "처럼";
    case "vir": return "의 그림자";
    case "lib": return `${wa} 같은`;
    case "sco": return `${eul} 좇는 자`;
    case "sag": return `${wa} 달리는`;
    case "cap": return "의 숨결";
    case "aqu": return `${eul} 부르는`;
    case "pis": return "의 노래";
    default:    return "의 노래";
  }
}

/**
 * 인디언식 이름 미리보기.
 * @param mbti  4글자 대문자(예: "ENTP")
 * @param blood 혈액형 대문자(예: "A", "AB")
 * @param zodiac 3글자 소문자 약어(예: "gem")
 * @returns 합성 이름 문자열, 축이 미선택이면 ""
 */
export function indianName(mbti: string, blood: string, zodiac: string): string {
  if (!mbti || !blood || !zodiac) return "";
  const adj = BLOOD_ADJ[blood.toUpperCase()];
  const noun = MBTI_NOUN[mbti.toUpperCase()];
  if (!adj || !noun) return "";
  const suffix = zodiacSuffix(zodiac.toLowerCase(), noun);
  return `${adj}${noun}${suffix}`;
}
