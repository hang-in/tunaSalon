// persona_kit.rs - 런타임 페르소나 조립 모듈
// 40개 조각(역할8 + MBTI16 + 혈액형4 + 별자리12)에서 on-demand 조립.
// precompute 없음. 순수 결정적. rng/네트워크/시간 없음.
// feature gate 없음(순수 Rust 데이터/로직).

use crate::model::{Persona, PersonaModifier};
use std::str::FromStr;

// ──────────────────────────────────────────────
// 1. 열거 타입 (40개 조각)
// ──────────────────────────────────────────────

/// 역할 8종 (persona-ui §2, §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Friend,
    Chaos,
    Critic,
    Realist,
    Teacher,
    Poet,
    Strategist,
    Summarizer,
}

impl Role {
    /// 역할 이름 문자열(소문자).
    pub fn key(self) -> &'static str {
        match self {
            Role::Friend => "friend",
            Role::Chaos => "chaos",
            Role::Critic => "critic",
            Role::Realist => "realist",
            Role::Teacher => "teacher",
            Role::Poet => "poet",
            Role::Strategist => "strategist",
            Role::Summarizer => "summarizer",
        }
    }

    /// §6 역할 기본 mu 값.
    pub fn base_mu(self) -> f64 {
        match self {
            Role::Friend => 0.80,
            Role::Chaos => 0.70,
            Role::Critic => 0.50,
            Role::Realist => 0.50,
            Role::Teacher => 0.45,
            Role::Poet => 0.35,
            Role::Strategist => 0.30,
            Role::Summarizer => 0.25,
        }
    }

    /// 역할 기본 PersonaModifier (reactivity, provocativeness).
    /// §6 "alpha 반응 대상"·"발화 제약" 성격 반영.
    pub fn base_modifier(self) -> PersonaModifier {
        match self {
            // friend: 전반·감정/긴장에 반응(높은 반응성), 자연스럽게 자극(중간 도발).
            Role::Friend => PersonaModifier { reactivity: 1.8, provocativeness: 1.2 },
            // chaos: 무작위·지루함에 반응(중간), 높은 도발성(엉뚱함이 자극).
            Role::Chaos => PersonaModifier { reactivity: 1.2, provocativeness: 1.8 },
            // critic: 주장·단정에 반응(높은 반응), 날카로운 도발(중간-높음).
            Role::Critic => PersonaModifier { reactivity: 1.6, provocativeness: 1.4 },
            // realist: 과장·비현실에 반응(중간), 낮은 도발(점검 톤).
            Role::Realist => PersonaModifier { reactivity: 1.2, provocativeness: 0.9 },
            // teacher: 질문·혼란에 반응(중간-높음), 낮은 도발(설명 위주).
            Role::Teacher => PersonaModifier { reactivity: 1.3, provocativeness: 0.8 },
            // poet: 감정·이미지에 반응(중간), 낮은 도발(비유).
            Role::Poet => PersonaModifier { reactivity: 1.0, provocativeness: 0.7 },
            // strategist: 방향 부재·교착에 반응(낮음), 낮은 도발(조용히 정리).
            Role::Strategist => PersonaModifier { reactivity: 0.8, provocativeness: 0.8 },
            // summarizer: 화제 누적 후 반응(매우 낮음), 매우 낮은 도발.
            Role::Summarizer => PersonaModifier { reactivity: 0.6, provocativeness: 0.5 },
        }
    }

    /// 역할 시스템 프롬프트 조각(핵심 기능 명시).
    pub fn prompt_fragment(self) -> &'static str {
        match self {
            Role::Friend => "You are a warm, easygoing friend in this group chat. React to the mood and feelings of the conversation.",
            Role::Chaos => "You are an unpredictable wildcard in this group chat. When things get dull or too serious, throw in something unexpected and playful.",
            Role::Critic => "You are a sharp-eyed critic in this group chat. When you hear overconfident claims or lazy assumptions, push back with a pointed observation.",
            Role::Realist => "You are a grounded realist in this group chat. When conversation drifts into wishful thinking or exaggeration, bring it back to earth with concrete facts.",
            Role::Teacher => "You are a patient explainer in this group chat. When there is confusion, a question, or a clear mistake, step in with a clear and helpful explanation.",
            Role::Poet => "You are a poetic soul in this group chat. When emotions or vivid images surface in the conversation, respond with an evocative metaphor or an unexpected angle.",
            Role::Strategist => "You are a quiet strategist in this group chat. When the discussion is going in circles or lacks direction, step in with a concise, structured suggestion.",
            Role::Summarizer => "You are a quiet observer in this group chat. After several exchanges have piled up, tie the loose threads together with a clear, brief summary.",
        }
    }

    /// 발화 제약 조각(§6 표 그대로).
    pub fn constraint_fragment(self) -> &'static str {
        match self {
            Role::Friend => "Keep your reply to 1-3 sentences, light and conversational.",
            Role::Chaos => "Keep it to 1 sentence. Make it unexpected.",
            Role::Critic => "Keep it to 1-2 sharp sentences.",
            Role::Realist => "Keep it to 1-2 sentences, grounded and matter-of-fact.",
            Role::Teacher => "Use 2-3 clear sentences to explain.",
            Role::Poet => "1-2 sentences with imagery or metaphor.",
            Role::Strategist => "Exactly 2 sentences: name the problem, give a direction.",
            Role::Summarizer => "1-2 sentences that tie things together.",
        }
    }

    /// 모든 역할 순서 목록(전수 테스트용).
    pub fn all() -> &'static [Role] {
        &[
            Role::Friend,
            Role::Chaos,
            Role::Critic,
            Role::Realist,
            Role::Teacher,
            Role::Poet,
            Role::Strategist,
            Role::Summarizer,
        ]
    }
}

impl FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "friend" => Ok(Role::Friend),
            "chaos" => Ok(Role::Chaos),
            "critic" => Ok(Role::Critic),
            "realist" => Ok(Role::Realist),
            "teacher" => Ok(Role::Teacher),
            "poet" => Ok(Role::Poet),
            "strategist" => Ok(Role::Strategist),
            "summarizer" => Ok(Role::Summarizer),
            other => Err(format!(
                "unknown role: \"{other}\". Valid: friend, chaos, critic, realist, teacher, poet, strategist, summarizer"
            )),
        }
    }
}

// ──────────────────────────────────────────────
// MBTI 16종
// ──────────────────────────────────────────────

/// MBTI 16종.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mbti {
    Entp, Entj, Enfp, Enfj,
    Estp, Estj, Esfp, Esfj,
    Intp, Intj, Infp, Infj,
    Istp, Istj, Isfp, Isfj,
}

impl Mbti {
    /// E/I 축 mu 보정. E +0.15, I -0.15 (§6 표).
    pub fn ei_mu_delta(self) -> f64 {
        match self {
            Mbti::Entp | Mbti::Entj | Mbti::Enfp | Mbti::Enfj |
            Mbti::Estp | Mbti::Estj | Mbti::Esfp | Mbti::Esfj => 0.15,
            _ => -0.15,
        }
    }

    /// T/F 축 modifier 보정. T: 주장/논리 반응 가중(reactivity+0.2, provocativeness+0.1).
    /// F: 감정 반응 가중(reactivity+0.1, provocativeness+0.2).
    pub fn tf_modifier_delta(self) -> (f64, f64) {
        match self {
            Mbti::Entp | Mbti::Entj | Mbti::Estp | Mbti::Estj |
            Mbti::Intp | Mbti::Intj | Mbti::Istp | Mbti::Istj => (0.2, 0.1),
            _ => (0.1, 0.2),
        }
    }

    /// N/S + J/P 말투/성향 조각(내용층 프롬프트 합류).
    pub fn style_fragment(self) -> &'static str {
        match self {
            // E variants
            Mbti::Entp => "You think out loud, love abstract connections, and often open new angles before closing the current one.",
            Mbti::Entj => "You think in abstractions and long-term patterns, and tend to wrap up threads decisively.",
            Mbti::Enfp => "You hop between ideas freely, drawn to abstract possibilities, always ready to spin off a new topic.",
            Mbti::Enfj => "You weave abstract meanings together and naturally guide conversations toward closure.",
            Mbti::Estp => "You stay grounded in the concrete and practical, pivoting fast when a topic runs dry.",
            Mbti::Estj => "You prefer concrete, actionable points and like to land on a definitive answer.",
            Mbti::Esfp => "You keep things vivid and present-tense, moving on freely when something new catches your eye.",
            Mbti::Esfj => "You focus on the concrete and personal, and gently steer conversations to a satisfying close.",
            // I variants
            Mbti::Intp => "You tend toward abstract theory and open questions, rarely in a hurry to wrap things up.",
            Mbti::Intj => "You think in sweeping abstractions and long arcs, and prefer tidy conclusions over loose ends.",
            Mbti::Infp => "You drift toward abstract ideals and inner meaning, happy to leave questions open.",
            Mbti::Infj => "You connect abstract patterns and deeper meaning, nudging conversations toward resolution.",
            Mbti::Istp => "You zero in on concrete mechanics and facts, leaving threads open if they lack a clear answer.",
            Mbti::Istj => "You anchor everything in concrete details and established facts, and prefer tying things off cleanly.",
            Mbti::Isfp => "You notice the sensory and personal, comfortable drifting without forcing a conclusion.",
            Mbti::Isfj => "You attend closely to concrete details and personal feelings, and try to bring things to a gentle close.",
        }
    }

    /// MBTI 4글자 문자열.
    pub fn code(self) -> &'static str {
        match self {
            Mbti::Entp => "ENTP", Mbti::Entj => "ENTJ",
            Mbti::Enfp => "ENFP", Mbti::Enfj => "ENFJ",
            Mbti::Estp => "ESTP", Mbti::Estj => "ESTJ",
            Mbti::Esfp => "ESFP", Mbti::Esfj => "ESFJ",
            Mbti::Intp => "INTP", Mbti::Intj => "INTJ",
            Mbti::Infp => "INFP", Mbti::Infj => "INFJ",
            Mbti::Istp => "ISTP", Mbti::Istj => "ISTJ",
            Mbti::Isfp => "ISFP", Mbti::Isfj => "ISFJ",
        }
    }

    /// 모든 MBTI 목록(전수 테스트용).
    pub fn all() -> &'static [Mbti] {
        &[
            Mbti::Entp, Mbti::Entj, Mbti::Enfp, Mbti::Enfj,
            Mbti::Estp, Mbti::Estj, Mbti::Esfp, Mbti::Esfj,
            Mbti::Intp, Mbti::Intj, Mbti::Infp, Mbti::Infj,
            Mbti::Istp, Mbti::Istj, Mbti::Isfp, Mbti::Isfj,
        ]
    }
}

impl FromStr for Mbti {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "ENTP" => Ok(Mbti::Entp), "ENTJ" => Ok(Mbti::Entj),
            "ENFP" => Ok(Mbti::Enfp), "ENFJ" => Ok(Mbti::Enfj),
            "ESTP" => Ok(Mbti::Estp), "ESTJ" => Ok(Mbti::Estj),
            "ESFP" => Ok(Mbti::Esfp), "ESFJ" => Ok(Mbti::Esfj),
            "INTP" => Ok(Mbti::Intp), "INTJ" => Ok(Mbti::Intj),
            "INFP" => Ok(Mbti::Infp), "INFJ" => Ok(Mbti::Infj),
            "ISTP" => Ok(Mbti::Istp), "ISTJ" => Ok(Mbti::Istj),
            "ISFP" => Ok(Mbti::Isfp), "ISFJ" => Ok(Mbti::Isfj),
            other => Err(format!("unknown MBTI: \"{other}\". Use 4-letter code, e.g. ENTP")),
        }
    }
}

// ──────────────────────────────────────────────
// 혈액형 4종
// ──────────────────────────────────────────────

/// 혈액형 4종. 내용층 캐릭터성 + 비주얼 팔레트.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blood {
    A,
    B,
    O,
    Ab,
}

impl Blood {
    /// 한국식 가벼운 캐릭터성 조각(내용층).
    pub fn character_fragment(self) -> &'static str {
        match self {
            Blood::A => "You have a meticulous, considerate side - you often double-check details and think about how others might feel.",
            Blood::B => "You have a free-spirited, self-directed side - you follow your own interest and don't mind going against the grain.",
            Blood::O => "You have an outgoing, passionate side - once you care about something you go all in and wear your heart on your sleeve.",
            Blood::Ab => "You have a dual nature - calm and analytical in one moment, suddenly playful or intense in the next.",
        }
    }

    /// 행동층 미세 보정 (mu_delta, reactivity_delta). 모두 ±0.05 이내.
    pub fn behavior_delta(self) -> (f64, f64) {
        match self {
            Blood::A  => (-0.02, -0.03),  // 신중 -> 약간 낮은 mu, 낮은 반응
            Blood::B  => ( 0.03,  0.04),  // 자유분방 -> 약간 높은 mu, 높은 반응
            Blood::O  => ( 0.04,  0.05),  // 외향·열정 -> 약간 높은 mu, 높은 반응
            Blood::Ab => (-0.01,  0.00),  // 이중성 -> 중립
        }
    }

    /// 비주얼 팔레트 주색(hex). 비주얼층 에셋 도착 시 사용.
    pub fn palette_hex(self) -> &'static str {
        match self {
            Blood::A  => "#6B8FD4",  // 차분한 블루
            Blood::B  => "#E0784A",  // 활기찬 오렌지
            Blood::O  => "#D44F4F",  // 열정적인 레드
            Blood::Ab => "#8C6BAE",  // 신비로운 퍼플
        }
    }

    /// 혈액형 코드 문자열.
    pub fn code(self) -> &'static str {
        match self {
            Blood::A => "A", Blood::B => "B", Blood::O => "O", Blood::Ab => "AB",
        }
    }

    /// 모든 혈액형 목록(전수 테스트용).
    pub fn all() -> &'static [Blood] {
        &[Blood::A, Blood::B, Blood::O, Blood::Ab]
    }
}

impl FromStr for Blood {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_uppercase().as_str() {
            "A"  => Ok(Blood::A),
            "B"  => Ok(Blood::B),
            "O"  => Ok(Blood::O),
            "AB" => Ok(Blood::Ab),
            other => Err(format!("unknown blood type: \"{other}\". Valid: a, b, o, ab")),
        }
    }
}

// ──────────────────────────────────────────────
// 별자리 12종
// ──────────────────────────────────────────────

/// 별자리 12종. 분위기 조각 + 비주얼 소품.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zodiac {
    Aries, Taurus, Gemini, Cancer,
    Leo, Virgo, Libra, Scorpio,
    Sagittarius, Capricorn, Aquarius, Pisces,
}

impl Zodiac {
    /// 분위기/감정선 조각(내용층).
    pub fn mood_fragment(self) -> &'static str {
        match self {
            Zodiac::Aries       => "You carry a bold, spark-first energy - you act before overthinking.",
            Zodiac::Taurus      => "You bring a steady, patient presence - you hold your ground and appreciate comfort.",
            Zodiac::Gemini      => "You have a restless, curious energy - your mood shifts and you love a good tangent.",
            Zodiac::Cancer      => "You carry a nurturing, intuitive undercurrent - emotional tones register strongly with you.",
            Zodiac::Leo         => "You have a warm, expressive flair - you enjoy the spotlight and lift the mood around you.",
            Zodiac::Virgo       => "You have a precise, slightly anxious attentiveness - you notice what others miss.",
            Zodiac::Libra       => "You carry a balanced, harmony-seeking energy - you weigh both sides before speaking.",
            Zodiac::Scorpio     => "You have an intense, probing undercurrent - you sense hidden dynamics and don't shy from depth.",
            Zodiac::Sagittarius => "You carry an optimistic, expansive energy - you push toward bigger ideas and farther horizons.",
            Zodiac::Capricorn   => "You have a dry, purposeful economy - you cut to what matters and skip the fluff.",
            Zodiac::Aquarius    => "You carry a detached, original perspective - you see things from an angle no one else considered.",
            Zodiac::Pisces      => "You have a dreamy, empathetic drift - you absorb the emotional atmosphere and respond in kind.",
        }
    }

    /// 행동층 미세 보정 (mu_delta, provocativeness_delta). 모두 ±0.05 이내.
    pub fn behavior_delta(self) -> (f64, f64) {
        match self {
            Zodiac::Aries       => ( 0.04,  0.05),
            Zodiac::Taurus      => (-0.03, -0.04),
            Zodiac::Gemini      => ( 0.03,  0.03),
            Zodiac::Cancer      => (-0.01,  0.01),
            Zodiac::Leo         => ( 0.04,  0.04),
            Zodiac::Virgo       => (-0.02, -0.02),
            Zodiac::Libra       => ( 0.00,  0.00),
            Zodiac::Scorpio     => ( 0.01,  0.05),
            Zodiac::Sagittarius => ( 0.03,  0.03),
            Zodiac::Capricorn   => (-0.03, -0.01),
            Zodiac::Aquarius    => ( 0.01,  0.02),
            Zodiac::Pisces      => (-0.02, -0.01),
        }
    }

    /// 비주얼 소품/심볼 이름. 비주얼층 에셋 슬롯용 데이터.
    pub fn prop_name(self) -> &'static str {
        match self {
            Zodiac::Aries       => "ram_horns",
            Zodiac::Taurus      => "bull_horns",
            Zodiac::Gemini      => "twin_stars",
            Zodiac::Cancer      => "crab_shell",
            Zodiac::Leo         => "lion_mane",
            Zodiac::Virgo       => "wheat_sprig",
            Zodiac::Libra       => "scales",
            Zodiac::Scorpio     => "scorpion_tail",
            Zodiac::Sagittarius => "arrow",
            Zodiac::Capricorn   => "goat_horn",
            Zodiac::Aquarius    => "water_wave",
            Zodiac::Pisces      => "fish_pair",
        }
    }

    /// 3글자 약어(parse_invite용). 영문 소문자.
    pub fn abbreviation(self) -> &'static str {
        match self {
            Zodiac::Aries       => "ari",
            Zodiac::Taurus      => "tau",
            Zodiac::Gemini      => "gem",
            Zodiac::Cancer      => "can",
            Zodiac::Leo         => "leo",
            Zodiac::Virgo       => "vir",
            Zodiac::Libra       => "lib",
            Zodiac::Scorpio     => "sco",
            Zodiac::Sagittarius => "sag",
            Zodiac::Capricorn   => "cap",
            Zodiac::Aquarius    => "aqu",
            Zodiac::Pisces      => "pis",
        }
    }

    /// 모든 별자리 목록(전수 테스트용).
    pub fn all() -> &'static [Zodiac] {
        &[
            Zodiac::Aries, Zodiac::Taurus, Zodiac::Gemini, Zodiac::Cancer,
            Zodiac::Leo, Zodiac::Virgo, Zodiac::Libra, Zodiac::Scorpio,
            Zodiac::Sagittarius, Zodiac::Capricorn, Zodiac::Aquarius, Zodiac::Pisces,
        ]
    }
}

impl FromStr for Zodiac {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "aries"  | "ari" => Ok(Zodiac::Aries),
            "taurus" | "tau" => Ok(Zodiac::Taurus),
            "gemini" | "gem" => Ok(Zodiac::Gemini),
            "cancer" | "can" => Ok(Zodiac::Cancer),
            "leo"            => Ok(Zodiac::Leo),
            "virgo"  | "vir" => Ok(Zodiac::Virgo),
            "libra"  | "lib" => Ok(Zodiac::Libra),
            "scorpio" | "sco" => Ok(Zodiac::Scorpio),
            "sagittarius" | "sag" => Ok(Zodiac::Sagittarius),
            "capricorn" | "cap" => Ok(Zodiac::Capricorn),
            "aquarius" | "aqu" => Ok(Zodiac::Aquarius),
            "pisces" | "pis" => Ok(Zodiac::Pisces),
            other => Err(format!(
                "unknown zodiac: \"{other}\". Use full name or 3-letter abbreviation (ari/tau/gem/can/leo/vir/lib/sco/sag/cap/aqu/pis)"
            )),
        }
    }
}

// ──────────────────────────────────────────────
// 2. 조립 출력 구조체
// ──────────────────────────────────────────────

/// 비주얼 슬롯 데이터. 픽셀아트 에셋 도착 시 렌더러가 사용.
/// 현재는 데이터 보존용(렌더 보류).
#[derive(Debug, Clone, PartialEq)]
pub struct VisualHint {
    /// 혈액형 주색(hex). 팔레트 스왑 시 사용.
    pub palette: String,
    /// 별자리 소품/심볼 이름. 오버레이 슬롯 키.
    pub prop: String,
}

/// 런타임 조립 결과. 세 층(행동/내용/비주얼) 완비.
#[derive(Debug, Clone, PartialEq)]
pub struct AssembledPersona {
    /// 엔진 행동층: id, name, base_rate(조립된 mu).
    pub persona: Persona,
    /// 내용층: 역할+MBTI+별자리+혈액형+발화제약 합성 시스템 프롬프트.
    pub system_prompt: String,
    /// 행동층: 역할 기본 modifier + MBTI T/F 보정.
    pub modifier: PersonaModifier,
    /// 비주얼층 슬롯(렌더 보류, 데이터만).
    pub visual: VisualHint,
}

// ──────────────────────────────────────────────
// 3. 조립 함수
// ──────────────────────────────────────────────

// ──────────────────────────────────────────────
// 인디언식 이름 자동 생성 (사람·페르소나 공용)
// ──────────────────────────────────────────────
//
// 룰(사용자 2026-06-03): 혈액형 -> 형용사, MBTI -> 자연/동물 명사, 별자리 -> 어미.
//   [혈액형 형용사][MBTI 명사][별자리 어미]를 붙여 한 이름으로.
//   예: O(평화로운) + ENTJ(태양) + Cancer(아래에서) = "평화로운태양아래에서".
// 결정적: 같은 3축 -> 같은 이름. 어미의 조사는 명사 받침에 따라 와/과·을/를 선택.

/// 마지막 글자에 받침(종성)이 있는가(한글 음절 기준).
fn has_batchim(s: &str) -> bool {
    s.chars()
        .last()
        .map(|c| {
            let u = c as u32;
            (0xAC00..=0xD7A3).contains(&u) && (u - 0xAC00) % 28 != 0
        })
        .unwrap_or(false)
}

/// 인디언식 이름(결정적): [혈액형 형용사][MBTI 명사][별자리 어미].
/// 사람·페르소나 공용(혈액형/MBTI/별자리 3축).
pub fn indian_name(mbti: Mbti, blood: Blood, zodiac: Zodiac) -> String {
    let adj = match blood.code() {
        "A" => "조용한",
        "B" => "지혜로운",
        "O" => "평화로운",
        _ => "날카로운", // AB
    };
    let noun = match mbti.code() {
        "ENTP" => "늑대", "ENTJ" => "태양", "ENFP" => "바람", "ENFJ" => "강",
        "ESTP" => "불꽃", "ESTJ" => "황소", "ESFP" => "나비", "ESFJ" => "하늘",
        "INTP" => "여우", "INTJ" => "매", "INFP" => "안개", "INFJ" => "달",
        "ISTP" => "곰", "ISTJ" => "산", "ISFP" => "사슴",
        _ => "별", // ISFJ
    };
    let bat = has_batchim(noun);
    let wa = if bat { "과" } else { "와" };
    let eul = if bat { "을" } else { "를" };
    let suffix = match zodiac.abbreviation() {
        "ari" => "의 기상".to_string(),
        "tau" => "처럼 우직한".to_string(),
        "gem" => format!("{wa} 함께 춤을"),
        "can" => "아래에서".to_string(),
        "leo" => "처럼".to_string(),
        "vir" => "의 그림자".to_string(),
        "lib" => format!("{wa} 같은"),
        "sco" => format!("{eul} 좇는 자"),
        "sag" => format!("{wa} 달리는"),
        "cap" => "의 숨결".to_string(),
        "aqu" => format!("{eul} 부르는"),
        _ => "의 노래".to_string(), // pis
    };
    // 닉네임 공백 제거(사용자 요청): "지혜로운 사슴과 달리는" -> "지혜로운사슴과달리는".
    format!("{adj}{noun}{suffix}").replace(' ', "")
}

/// 이름에서 결정적 id slug 생성.
/// 소문자 변환 + 공백/특수문자를 '_'로 치환 + 4글자 이하 축약어 뒤에 역할 코드 추가.
fn make_id(name: &str, role: Role) -> String {
    let slug: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect();
    let slug = slug.trim_matches('_').to_string();
    if slug.is_empty() {
        format!("{}_{}", role.key(), "persona")
    } else {
        slug
    }
}

/// 4축 조합으로 페르소나를 런타임 조립한다.
///
/// base_rate = 역할 mu + MBTI E/I 보정 + 혈액형 mu_delta + 별자리 mu_delta, clamp [0.05, 0.98].
/// modifier  = 역할 기본 + MBTI T/F delta + 별자리 provocativeness_delta + 혈액형 reactivity_delta.
/// system_prompt 합성 순서: 역할 -> MBTI 말투 -> 별자리 분위기 -> 혈액형 캐릭터성 -> 발화 제약.
pub fn assemble(role: Role, mbti: Mbti, blood: Blood, zodiac: Zodiac, name: &str) -> AssembledPersona {
    // 이름이 비어 있으면 인디언식으로 자동 생성(임의 입력 없을 때; 이름은 축에서 결정).
    let name_owned = if name.trim().is_empty() {
        indian_name(mbti, blood, zodiac)
    } else {
        name.to_string()
    };
    let name = name_owned.as_str();

    // --- 행동층 ---
    let (blood_mu_delta, blood_reactivity_delta) = blood.behavior_delta();
    let (zodiac_mu_delta, zodiac_prov_delta) = zodiac.behavior_delta();
    let (tf_react_delta, tf_prov_delta) = mbti.tf_modifier_delta();

    let raw_mu = role.base_mu()
        + mbti.ei_mu_delta()
        + blood_mu_delta
        + zodiac_mu_delta;
    let base_rate = raw_mu.clamp(0.05, 0.98);

    let base_mod = role.base_modifier();
    let modifier = PersonaModifier {
        reactivity: (base_mod.reactivity + tf_react_delta + blood_reactivity_delta).max(0.1),
        provocativeness: (base_mod.provocativeness + tf_prov_delta + zodiac_prov_delta).max(0.1),
    };

    // --- 내용층(system_prompt 합성) ---
    // 순서: 역할 -> MBTI 말투 -> 별자리 분위기 -> 혈액형 캐릭터성 -> 발화 제약 -> 언어 지시.
    // 언어 지시($LANG 기반, 기본 한국어): 없으면 동적 초대 persona가 영어로 답하는 버그.
    // 기존 데모 3인(demo_persona_system_prompts)과 동일하게 locale::reply_language()를 쓴다.
    let lang = crate::locale::reply_language();
    let system_prompt = format!(
        "You are {name}. {role_prompt} {mbti_style} {zodiac_mood} {blood_char} {constraint} React to what the others JUST said and build on it; do not ignore them or keep pushing your own topic, and do not drift into unrelated tangents. Never repeat, agree with, or react to your OWN earlier line as if someone else said it. When 나 says something, answer 나 directly and follow their lead. Always respond in {lang}, even if others write in another language. When asked your name, answer {name}.",
        name = name,
        role_prompt  = role.prompt_fragment(),
        mbti_style   = mbti.style_fragment(),
        zodiac_mood  = zodiac.mood_fragment(),
        blood_char   = blood.character_fragment(),
        constraint   = role.constraint_fragment(),
        lang = lang,
    );

    // --- id ---
    let id = make_id(name, role);

    // --- 비주얼층 ---
    let visual = VisualHint {
        palette: blood.palette_hex().to_string(),
        prop: zodiac.prop_name().to_string(),
    };

    AssembledPersona {
        persona: Persona { id, name: name.to_string(), base_rate },
        system_prompt,
        modifier,
        visual,
    }
}

// ──────────────────────────────────────────────
// 4. /invite 파싱
// ──────────────────────────────────────────────

/// `/invite` 명령 파싱 결과.
#[derive(Debug, Clone)]
pub struct InviteSpec {
    /// 사용할 LLM 모델 이름(예: "gemma4:e4b").
    pub model: String,
    /// 조립된 페르소나.
    pub assembled: AssembledPersona,
}

/// "/invite" 뒤 인자 문자열을 파싱한다.
///
/// 형식: `<model> <MBTI> <blood> <zodiac(3글자)> <role> <name>`
/// 예:   `gemma4:e4b entp b sag critic 입털이`
///
/// 오류 시 Err(설명 문자열) 반환.
pub fn parse_invite(args: &str) -> Result<InviteSpec, String> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 6 {
        return Err(format!(
            "invite requires 6 arguments: <model> <MBTI> <blood> <zodiac> <role> <name>. Got {} token(s): \"{}\"",
            parts.len(), args
        ));
    }

    let model  = parts[0].to_string();
    let mbti   = Mbti::from_str(parts[1])?;
    let blood  = Blood::from_str(parts[2])?;
    let zodiac = Zodiac::from_str(parts[3])?;
    let role   = Role::from_str(parts[4])?;
    // 이름은 나머지 토큰 전부 합침(공백 포함 이름 허용).
    let name   = parts[5..].join(" ");

    let assembled = assemble(role, mbti, blood, zodiac, &name);
    Ok(InviteSpec { model, assembled })
}

// ──────────────────────────────────────────────
// 5. 단위 테스트
// ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // --- 결정성 ---
    #[test]
    fn assemble_is_deterministic() {
        let a1 = assemble(Role::Critic, Mbti::Entp, Blood::B, Zodiac::Scorpio, "입털이");
        let a2 = assemble(Role::Critic, Mbti::Entp, Blood::B, Zodiac::Scorpio, "입털이");
        assert_eq!(a1.persona.id,     a2.persona.id);
        assert_eq!(a1.persona.name,   a2.persona.name);
        assert_eq!(a1.persona.base_rate, a2.persona.base_rate);
        assert_eq!(a1.system_prompt,  a2.system_prompt);
        assert_eq!(a1.modifier,       a2.modifier);
        assert_eq!(a1.visual,         a2.visual);
    }

    // --- MBTI E/I 보정 ---
    #[test]
    fn extrovert_has_higher_base_rate_than_introvert() {
        // 같은 역할, T/F/N/J 고정, E vs I 만 다름
        let e = assemble(Role::Realist, Mbti::Entj, Blood::O, Zodiac::Leo, "e_test");
        let i = assemble(Role::Realist, Mbti::Intj, Blood::O, Zodiac::Leo, "i_test");
        assert!(
            e.persona.base_rate > i.persona.base_rate,
            "ENTJ base_rate({}) should be > INTJ base_rate({})",
            e.persona.base_rate, i.persona.base_rate
        );
    }

    // E/I delta가 정확히 0.30 차이여야 함(+0.15 vs -0.15, 다른 조건 동일)
    #[test]
    fn ei_delta_is_exactly_0_30() {
        let e = assemble(Role::Teacher, Mbti::Estp, Blood::A, Zodiac::Libra, "e");
        let i = assemble(Role::Teacher, Mbti::Istp, Blood::A, Zodiac::Libra, "i");
        let diff = e.persona.base_rate - i.persona.base_rate;
        assert!(
            (diff - 0.30).abs() < 1e-10,
            "E/I diff should be 0.30, got {diff}"
        );
    }

    // --- 역할 mu 반영 ---
    #[test]
    fn friend_has_higher_base_rate_than_summarizer_same_mbti() {
        let f = assemble(Role::Friend,    Mbti::Infp, Blood::A, Zodiac::Pisces, "f");
        let s = assemble(Role::Summarizer, Mbti::Infp, Blood::A, Zodiac::Pisces, "s");
        assert!(
            f.persona.base_rate > s.persona.base_rate,
            "friend({}) should be > summarizer({})",
            f.persona.base_rate, s.persona.base_rate
        );
    }

    // --- clamp ---
    #[test]
    fn base_rate_is_clamped_within_bounds() {
        // 모든 역할 x MBTI 조합 중 극단값에서도 clamp 확인
        for &role in Role::all() {
            for &mbti in Mbti::all() {
                for &blood in Blood::all() {
                    for &zodiac in Zodiac::all() {
                        let a = assemble(role, mbti, blood, zodiac, "test");
                        assert!(
                            a.persona.base_rate >= 0.05 && a.persona.base_rate <= 0.98,
                            "base_rate out of bounds: {} for {:?}/{:?}/{:?}/{:?}",
                            a.persona.base_rate, role, mbti, blood, zodiac
                        );
                    }
                }
            }
        }
    }

    // --- system_prompt 합성: 역할 핵심어 + 발화 제약 포함 ---
    #[test]
    fn system_prompt_contains_role_keyword_and_constraint() {
        let a = assemble(Role::Critic, Mbti::Intj, Blood::Ab, Zodiac::Scorpio, "날카론이");
        // 역할 프롬프트 핵심어 확인
        assert!(
            a.system_prompt.contains("critic"),
            "prompt should contain 'critic': {}", a.system_prompt
        );
        // 발화 제약 핵심어 확인
        assert!(
            a.system_prompt.contains("sharp"),
            "prompt should contain constraint keyword 'sharp': {}", a.system_prompt
        );
    }

    #[test]
    fn system_prompt_contains_friend_role_and_constraint() {
        let a = assemble(Role::Friend, Mbti::Esfj, Blood::O, Zodiac::Leo, "따뜻이");
        assert!(
            a.system_prompt.contains("warm"),
            "friend prompt should contain 'warm': {}", a.system_prompt
        );
        assert!(
            a.system_prompt.contains("1-3 sentences"),
            "friend constraint should appear: {}", a.system_prompt
        );
    }

    // --- parse_invite 정상 케이스 ---
    #[test]
    fn parse_invite_valid_basic() {
        let spec = parse_invite("gemma4:e4b entp b sag critic 입털이").unwrap();
        assert_eq!(spec.model, "gemma4:e4b");
        assert_eq!(spec.assembled.persona.name, "입털이");
        assert_eq!(spec.assembled.persona.base_rate.clamp(0.05, 0.98), spec.assembled.persona.base_rate);
    }

    #[test]
    fn parse_invite_mbti_blood_zodiac_role_fields() {
        let spec = parse_invite("gemma4:31b-cloud isfj a tau friend 걱정봇").unwrap();
        assert_eq!(spec.model, "gemma4:31b-cloud");
        assert_eq!(spec.assembled.persona.name, "걱정봇");
        // friend 역할이므로 system_prompt에 warm 포함
        assert!(spec.assembled.system_prompt.contains("warm"));
        // Taurus 분위기 포함
        assert!(spec.assembled.system_prompt.contains("steady"));
    }

    #[test]
    fn parse_invite_multi_word_name() {
        let spec = parse_invite("gemma4:e4b entp o leo chaos 입 털 이").unwrap();
        assert_eq!(spec.assembled.persona.name, "입 털 이");
    }

    #[test]
    fn parse_invite_too_few_args_returns_err() {
        assert!(parse_invite("gemma4:e4b entp b sag critic").is_err());
        assert!(parse_invite("").is_err());
        assert!(parse_invite("model only").is_err());
    }

    #[test]
    fn parse_invite_invalid_mbti_returns_err() {
        assert!(parse_invite("model XXXX b sag critic 이름").is_err());
    }

    #[test]
    fn parse_invite_invalid_blood_returns_err() {
        assert!(parse_invite("model entp C sag critic 이름").is_err());
    }

    #[test]
    fn parse_invite_invalid_zodiac_returns_err() {
        assert!(parse_invite("model entp b xxx critic 이름").is_err());
    }

    #[test]
    fn parse_invite_invalid_role_returns_err() {
        assert!(parse_invite("model entp b sag wizard 이름").is_err());
    }

    // --- case insensitive 파싱 ---
    #[test]
    fn parse_invite_case_insensitive() {
        let a = parse_invite("Model ENTP B SAG CRITIC 이름").unwrap();
        let b = parse_invite("model entp b sag critic 이름").unwrap();
        assert_eq!(a.assembled.persona.base_rate, b.assembled.persona.base_rate);
        assert_eq!(a.assembled.system_prompt, b.assembled.system_prompt);
    }

    // --- 40조각 전수: 8역할 x 16MBTI x 4혈액형 x 12별자리 모두 패닉 없이 동작 ---
    #[test]
    fn all_combinations_assemble_without_panic() {
        let mut count = 0usize;
        for &role in Role::all() {
            for &mbti in Mbti::all() {
                for &blood in Blood::all() {
                    for &zodiac in Zodiac::all() {
                        let a = assemble(role, mbti, blood, zodiac, "test_persona");
                        assert!(!a.system_prompt.is_empty());
                        assert!(!a.persona.id.is_empty());
                        count += 1;
                    }
                }
            }
        }
        // 8 * 16 * 4 * 12 = 6144
        assert_eq!(count, 6144);
    }

    // --- 샘플 몇 개: 각 enum 구석 케이스 ---
    #[test]
    fn sample_combinations_check() {
        // 역할 마지막 - 별자리 마지막 - 혈액형 마지막 - MBTI 마지막
        let a = assemble(Role::Summarizer, Mbti::Isfj, Blood::Ab, Zodiac::Pisces, "샘플");
        assert!(a.persona.base_rate >= 0.05 && a.persona.base_rate <= 0.98);
        assert!(a.system_prompt.contains("Summarizer") || a.system_prompt.contains("observer"));

        // 역할 첫 번째 - 별자리 첫 번째
        let b = assemble(Role::Friend, Mbti::Entp, Blood::A, Zodiac::Aries, "샘플2");
        assert!(b.system_prompt.contains("warm"));

        // Strategist + INTJ
        let c = assemble(Role::Strategist, Mbti::Intj, Blood::B, Zodiac::Capricorn, "참모");
        assert!(c.system_prompt.contains("strategist") || c.system_prompt.contains("Strategist") || c.system_prompt.contains("direction"));
    }

    // --- role FromStr ---
    #[test]
    fn role_from_str_all_valid() {
        for &role in Role::all() {
            let parsed = Role::from_str(role.key()).unwrap();
            assert_eq!(parsed, role);
        }
        assert!(Role::from_str("invalid").is_err());
    }

    // --- mbti FromStr ---
    #[test]
    fn mbti_from_str_all_valid() {
        for &mbti in Mbti::all() {
            let parsed = Mbti::from_str(mbti.code()).unwrap();
            assert_eq!(parsed, mbti);
        }
        assert!(Mbti::from_str("ABCD").is_err());
    }

    // --- blood FromStr ---
    #[test]
    fn blood_from_str_all_valid() {
        for &blood in Blood::all() {
            let parsed = Blood::from_str(blood.code()).unwrap();
            assert_eq!(parsed, blood);
        }
        assert!(Blood::from_str("C").is_err());
    }

    // --- zodiac FromStr ---
    #[test]
    fn zodiac_from_str_abbreviations() {
        for &zodiac in Zodiac::all() {
            let parsed = Zodiac::from_str(zodiac.abbreviation()).unwrap();
            assert_eq!(parsed, zodiac);
        }
        assert!(Zodiac::from_str("xyz").is_err());
    }

    // --- modifier 합리성 검증: friend > summarizer reactivity ---
    #[test]
    fn friend_has_higher_reactivity_than_summarizer() {
        let f = assemble(Role::Friend,    Mbti::Estp, Blood::O, Zodiac::Leo, "f");
        let s = assemble(Role::Summarizer, Mbti::Estp, Blood::O, Zodiac::Leo, "s");
        assert!(
            f.modifier.reactivity > s.modifier.reactivity,
            "friend reactivity({}) should be > summarizer({})",
            f.modifier.reactivity, s.modifier.reactivity
        );
    }

    // --- visual hint 필드 비어있지 않음 ---
    #[test]
    fn visual_hint_fields_are_nonempty() {
        let a = assemble(Role::Poet, Mbti::Infp, Blood::Ab, Zodiac::Pisces, "시인");
        assert!(!a.visual.palette.is_empty());
        assert!(!a.visual.prop.is_empty());
        assert!(a.visual.palette.starts_with('#'));
    }

    // ── 인디언식 이름 ────────────────────────────────

    /// 사용자 명시 예: O(평화로운) + ENTJ(태양) + Cancer(아래에서) = "평화로운태양아래에서".
    #[test]
    fn indian_name_matches_user_example() {
        let m: Mbti = "entj".parse().unwrap();
        let b: Blood = "o".parse().unwrap();
        let z: Zodiac = "can".parse().unwrap();
        assert_eq!(indian_name(m, b, z), "평화로운태양아래에서");
    }

    /// 결정적: 같은 3축 -> 같은 이름.
    #[test]
    fn indian_name_deterministic() {
        let m: Mbti = "entp".parse().unwrap();
        let b: Blood = "a".parse().unwrap();
        let z: Zodiac = "gem".parse().unwrap();
        assert_eq!(indian_name(m, b, z), indian_name(m, b, z));
    }

    /// 받침 조사: 받침 있는 명사는 "과", 없는 명사는 "와".
    #[test]
    fn indian_name_josa_by_batchim() {
        // ENTP=늑대(받침X) + Gemini(gem, ~와 함께 춤을) -> "와"
        let wolf: Mbti = "entp".parse().unwrap();
        let a: Blood = "a".parse().unwrap();
        let gem: Zodiac = "gem".parse().unwrap();
        assert!(indian_name(wolf, a, gem).contains("와함께춤을"));
        // ESTJ=황소(받침X)도 "와"; ISTJ=산(받침O) -> "과"
        let mountain: Mbti = "istj".parse().unwrap();
        assert!(indian_name(mountain, a, gem).contains("과함께춤을"));
    }

    /// 빈 이름이면 인디언식 자동 생성.
    #[test]
    fn assemble_empty_name_autogenerates() {
        let m: Mbti = "enfp".parse().unwrap();
        let b: Blood = "b".parse().unwrap();
        let z: Zodiac = "leo".parse().unwrap();
        let r: Role = "friend".parse().unwrap();
        let p = assemble(r, m, b, z, "");
        assert!(!p.persona.name.trim().is_empty(), "빈 이름이면 자동 생성되어야 한다");
        assert_eq!(p.persona.name, "지혜로운바람처럼"); // B(지혜로운)+ENFP(바람)+leo(처럼)
    }
}
