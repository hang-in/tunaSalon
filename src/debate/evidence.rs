//! 루프 차단용 twist/evidence card(순수). 토론이 고착·과수렴하면 새 국면을 1장 투입한다.
//!
//! 모드별 카드 풀에서 결정적으로 고른다(rng 무소비). 토픽별 실데이터 카드(COMPAS 등)는
//! 후속(토픽 카드 데이터 필요) — Stage E.1은 모드 수준의 반전 프롬프트로 충분히 흔든다.
//! history_snapshot(복제본)에만 주입되어 state/golden 불변.

use super::plan::DebateMode;

/// 모드와 회전 인덱스로 새 국면(twist) 카드 하나를 고른다.
pub(crate) fn twist_card(mode: DebateMode, idx: usize) -> &'static str {
    let pool = mode_cards(mode);
    pool[idx % pool.len()]
}

fn mode_cards(mode: DebateMode) -> &'static [&'static str] {
    match mode {
        DebateMode::Courtroom => &[
            "[새 국면] 같은 법원에서 인간 판사의 양형 편차가 통계로 드러났다고 합시다. 이 사실이 당신 주장을 어떻게 바꾸거나 강화하는지 답하세요.",
            "[새 국면] AI의 설명이 틀려 오판이 났던 사례가 보고됐습니다. 이를 반영해 당신 입장을 방어하거나 수정하세요.",
        ],
        DebateMode::PolicyDuel => &[
            "[새 국면] 자원봉사 유지보수자가 규제 부담으로 프로젝트를 접었다는 사례가 나왔습니다. 당신 입장은 이 비용을 누가 어떻게 감당한다고 봅니까?",
            "[새 국면] 규제가 없던 영역에서 큰 사고가 터졌다고 합시다. 자율 vs 강제 중 어디를 택할지 다시 답하세요.",
        ],
        DebateMode::MoralDilemma => &[
            "[새 국면] 당신이 옹호한 선택으로 한 개인이 회복 불가능한 피해를 봤다고 합시다. 그래도 입장을 유지합니까?",
            "[새 국면] 반대편 가치를 우선했을 때의 구체적 이득을 인정한 뒤, 그럼에도 당신 가치를 택하는 이유를 대세요.",
        ],
        DebateMode::PersonalStakes => &[
            "[새 국면] 그 선택이 당신과 가장 가까운 사람을 외롭게 만든다면 어떻게 하겠습니까? 구체적으로 답하세요.",
            "[새 국면] 정반대로 선택한 사람의 실제 후회담이 나왔다고 합시다. 당신 입장에 어떤 영향이 있습니까?",
        ],
        DebateMode::Forecasting => &[
            "[새 국면] 당신 예측과 반대되는 초기 신호가 관측됐다고 합시다. 예측을 수정합니까, 방어합니까?",
            "[새 국면] 무엇을 보면 당신이 틀렸다고 인정할지 측정 가능한 신호 하나를 못 박으세요.",
        ],
        DebateMode::DesignReview => &[
            "[새 국면] 당신 설계가 부하 10배에서 무너졌다고 합시다. 어디를 포기하고 어디를 지키겠습니까?",
            "[새 국면] 사용자가 가장 자주 쓰는 경로에서 당신 설계가 가장 느립니다. 트레이드오프를 다시 정당화하세요.",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twist_card_is_mode_specific_and_deterministic() {
        let a = twist_card(DebateMode::Courtroom, 0);
        assert!(a.starts_with("[새 국면]"));
        assert_eq!(a, twist_card(DebateMode::Courtroom, 0));
        // 회전: 인덱스가 풀 크기를 넘어가면 다시 처음으로
        assert_eq!(twist_card(DebateMode::Courtroom, 0), twist_card(DebateMode::Courtroom, 2));
        // 모드별로 다른 카드
        assert_ne!(
            twist_card(DebateMode::Courtroom, 0),
            twist_card(DebateMode::PolicyDuel, 0)
        );
    }
}
