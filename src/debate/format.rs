//! 발화 형식/길이 변주 힌트(순수).
//!
//! plan이 있으면 모드에 맞는 *형식* 풀(교차신문/스틸맨/사례/임계값…)에서 고르고,
//! 없으면 기존 길이 변주로 폴백한다. tick+화자 기반 결정적 선택이라 **rng를 소비하지
//! 않는다**(골든·화자선택 결정성 무영향). history_snapshot(복제본)에만 주입.

use super::plan::{DebateMode, DebatePlan};

/// 발화 형식 변주 힌트(생성 워커 프롬프트용).
pub(crate) fn format_hint(tick: u64, speaker: &str, plan: Option<&DebatePlan>) -> String {
    let salt: usize = speaker.bytes().map(|b| b as usize).sum();
    let idx = (tick as usize).wrapping_add(salt);
    match plan {
        None => length_hint(idx).to_string(),
        Some(p) => {
            let pool = mode_formats(p.mode);
            pool[idx % pool.len()].to_string()
        }
    }
}

// ── 형식 스니펫(핸드오프 UtteranceFormat 차용) ────────────────────────────────
// 각 형식은 *길이 성격*도 함께 지정해 발화가 일률적으로 길어지지 않게 한다(단조로움 방지).
const CROSS_EXAM: &str =
    "[형식: 교차신문 · 짧게] 한두 문장으로. 날카로운 질문 하나만 툭 던지고 왜 중요한지만 붙여. 길게 늘이지 마.";
const STEELMAN: &str =
    "[형식: 스틸맨 후 반박 · 중간] 상대의 제일 센 논점을 한 문장으로 요약해준 뒤, 전제 하나를 두세 문장으로 받아쳐.";
const CONCRETE_CASE: &str =
    "[형식: 사례 · 중간] 구체적인 사례 하나만 서너 문장으로 풀어서 네 편을 세워. 두 번째 사례는 붙이지 마.";
const THRESHOLD: &str =
    "[형식: 기준 · 짧게] 두세 문장으로. 측정 가능한 기준이나 '이러면 접는다' 조건 하나만 던져.";
const CONCESSION: &str =
    "[형식: 조건부 양보 · 짧게] 한두 문장으로. 뭐가 충족되면 입장 바꿀지만 분명히 말해.";
const DILEMMA_FORK: &str =
    "[형식: 양자택일 · 짧게] 짧게. 두 선택지의 트레이드오프를 세우고 상대보고 골라보라 그래.";
const DIRECT_REBUTTAL: &str =
    "[형식: 직접 반박 · 짧게] 두세 문장으로. 바로 앞 말의 한 군데만 콕 집어 닉네임 부르며 받아쳐.";
const OPENING: &str =
    "[형식: 입장 표명 · 짧게] 네 입장을 한 문장으로 딱 정하고 이유 하나만 붙여.";
const PREDICTION: &str =
    "[형식: 예측 · 짧게] 예측이랑 확신도를 짧게 말하고, 뭘 보면 틀린 건지 한 문장 덧붙여.";

/// 모드별 형식 풀. tick+화자로 회전 선택된다.
fn mode_formats(mode: DebateMode) -> &'static [&'static str] {
    match mode {
        DebateMode::Courtroom => &[CROSS_EXAM, STEELMAN, CONCRETE_CASE, THRESHOLD],
        DebateMode::PolicyDuel => &[THRESHOLD, CONCESSION, DIRECT_REBUTTAL, DILEMMA_FORK],
        DebateMode::MoralDilemma => &[DILEMMA_FORK, STEELMAN, CONCESSION, CONCRETE_CASE],
        DebateMode::PersonalStakes => &[CONCRETE_CASE, DIRECT_REBUTTAL, OPENING, CONCESSION],
        DebateMode::Forecasting => &[PREDICTION, THRESHOLD, CONCESSION, DIRECT_REBUTTAL],
        DebateMode::DesignReview => &[DIRECT_REBUTTAL, DILEMMA_FORK, THRESHOLD, STEELMAN],
        DebateMode::CasualBanter => &[CASUAL_PICK, CASUAL_JAB, CASUAL_EXAMPLE, CASUAL_EXAGGERATE],
    }
}

// ── 캐주얼(반말) 형식 — 음식·취향 등 가벼운 주제용 ──────────────────────────────
const CASUAL_PICK: &str =
    "[형식: 취향 선언 · 반말] 네 취향을 한 문장으로 딱 정하고, 왜 그런지 웃긴 이유 하나만 대.";
const CASUAL_JAB: &str =
    "[형식: 받아치기 · 반말] 바로 앞 사람 닉네임 부르며 한두 문장으로 가볍게 받아쳐.";
const CASUAL_EXAMPLE: &str =
    "[형식: 경험담 · 반말] 짧은 경험담 하나로 네 편을 들어. 길게 늘이지 마.";
const CASUAL_EXAGGERATE: &str =
    "[형식: 우기기 · 반말] 살짝 오버해서 네 취향을 유쾌하게 우겨. 진지하게 분석하지 마.";

/// plan 없는 세션(일반 `--chat`)용 길이 변주 폴백. `idx`는 (tick+화자 salt).
/// Stage D 이전 `length_hint(tick, speaker)`와 동일 동작(behavior-preserving).
fn length_hint(idx: usize) -> &'static str {
    match idx % 4 {
        0 => "[길이] 한두 문장으로 가볍게. 네 입장이랑 상대 말에 어떻게 연결되는지만 툭.",
        1 => "[길이] 두세 문장으로. 찬반 입장 분명히 하고 반례나 조건 하나만 던져.",
        2 => "[길이] 두세 문장으로. 상대 닉네임 부르며 핵심 전제 하나만 짚어.",
        _ => "[길이] 한두 문장으로 짧게. 강의하지 말고 친구한테 말하듯 가볍게 네 편을 들어.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debate::infer_debate_plan;

    #[test]
    fn none_plan_falls_back_to_length_hint() {
        let h = format_hint(0, "a", None);
        assert!(h.starts_with("[길이]"));
    }

    #[test]
    fn plan_uses_mode_format_pool() {
        let plan = infer_debate_plan(&["AI 판사가 공정할까?".to_string()]); // Courtroom
        let h = format_hint(0, "a", Some(&plan));
        assert!(h.starts_with("[형식:"));
    }

    #[test]
    fn deterministic_no_rng() {
        let plan = infer_debate_plan(&["AI 규제와 오픈소스".to_string()]);
        assert_eq!(
            format_hint(7, "지혜로운바람", Some(&plan)),
            format_hint(7, "지혜로운바람", Some(&plan))
        );
    }
}
