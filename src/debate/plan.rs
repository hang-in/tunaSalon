//! DebatePlan: 주제로부터 토론의 *종류*를 결정적으로 추론한다(LLM 미사용).
//!
//! Stage B(2026-06-26): 타입 + 추론 + 테스트만. `LiveSession` 배선은 Stage C/D.
//! 순수(rng·IO·상태 없음) → 골든 무영향. 같은 주제 → 같은 plan(결정적).
//!
//! 추론은 키워드 점유(substring 출현 횟수) 스코어링이다. 모드별 키워드 집합에서
//! 최고 점수 모드를 고르고, 동점이면 우선순위(가장 구체적인 모드 먼저)로 깬다.

/// 토론의 종류. 같은 주제라도 모드에 따라 연출(지시·형식·증거)이 달라진다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebateMode {
    /// 법·규제·제도·정책 다툼.
    PolicyDuel,
    /// 가치·권리·존엄의 트레이드오프.
    MoralDilemma,
    /// 주장마다 근거를 대고 교차신문하는 법정형.
    Courtroom,
    /// 예측·확률·신뢰도를 내거는 전망형.
    Forecasting,
    /// 제품·시스템 설계 논쟁.
    DesignReview,
    /// 연애·가족·친구 등 일상에 발 디딘 개인적 쟁점.
    PersonalStakes,
    /// 음식·취향 등 가볍고 호불호 갈리는 주제. 반말로 유쾌하게 티격태격.
    CasualBanter,
}

/// 주제에서 추론한 토론 연출 계획(결정적). v1은 mode/opening/stakes/fault_lines까지.
/// 숨은목표(역할)·format cycle·evidence card는 Stage D/E에서 채운다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebatePlan {
    /// 표시용 주제 문자열(여러 토픽은 ", "로 결합).
    pub topic: String,
    pub mode: DebateMode,
    /// 사회자형 개막 질문(엔진이 첫 화자를 고르도록 유도).
    pub opening_question: String,
    /// 무엇이 걸려 있는가(한 줄).
    pub stakes: String,
    /// 대립축(참가자가 어디서 갈리는지) 3~4개.
    pub fault_lines: Vec<String>,
}

impl DebateMode {
    /// 짧은 모드 라벨(생성 지시용).
    pub(crate) fn label(self) -> &'static str {
        match self {
            DebateMode::PolicyDuel => "정책 다툼",
            DebateMode::MoralDilemma => "가치 딜레마",
            DebateMode::Courtroom => "법정형 다툼",
            DebateMode::Forecasting => "전망·예측",
            DebateMode::DesignReview => "설계 논쟁",
            DebateMode::PersonalStakes => "개인적 쟁점",
            DebateMode::CasualBanter => "가벼운 취향 논쟁",
        }
    }

    /// 주제가 다른 영역으로 새지 않게 잡아주는 모드별 행동 지시(anti-drift).
    pub(crate) fn instruction(self) -> &'static str {
        match self {
            DebateMode::PolicyDuel => {
                "누가 뭘 의무화·금지하고 그 비용·집행은 누가 지는지 가볍게 따져봐. 보고서 쓰듯 말고 친구끼리 투닥거리듯 편하게."
            }
            DebateMode::MoralDilemma => {
                "각자 뭘 더 중요하게 보는지 솔직히 까고 그 대가도 인정하면서, 친구끼리 진솔하게 떠들듯 얘기해."
            }
            DebateMode::Courtroom => {
                "공정성·책임이 쟁점이야. 근거 하나 대고 상대 전제를 콕 찔러보되, 법정처럼 딱딱하게 말고 친구끼리 따지듯 가볍게."
            }
            DebateMode::Forecasting => {
                "어떻게 될지 예측하고 얼마나 확신하는지 걸어봐. 뭘 보면 누가 틀린 건지도 가볍게 곁들이고, 편하게."
            }
            DebateMode::DesignReview => {
                "대안 하나 던지고 그게 어디서 좋고 어디서 터지는지 편하게 짚어. 설계 리뷰지만 친구끼리 훈수 두듯 가볍게."
            }
            DebateMode::PersonalStakes => {
                "거창한 정책 얘기로 빠지지 말고, 네 경험에서 출발해 친구한테 털어놓듯 편하게 입장을 잡아."
            }
            DebateMode::CasualBanter => {
                "존댓말 말고 반말로, 가볍고 유쾌하게 티격태격하세요. 진지한 정책·윤리 분석은 빼고 \
                 각자 취향과 웃긴 근거로 우기되 상대를 무시하진 마세요."
            }
        }
    }
}

impl DebatePlan {
    /// 생성 워커에 주입할 압축 토론 프레임(한 줄). tick으로 대립축을 회전 선택한다.
    /// 길게 쓰면 모델이 드리프트하므로 의도적으로 짧게 유지(라벨+anti-drift 지시+대립축 1개).
    pub(crate) fn directive_line(&self, tick: u64) -> String {
        let fault = if self.fault_lines.is_empty() {
            String::new()
        } else {
            format!(
                " (대립축: {})",
                self.fault_lines[(tick as usize) % self.fault_lines.len()]
            )
        };
        format!(
            "[토론] 이 쟁점은 {} 성격입니다 - {}{}",
            self.mode.label(),
            self.mode.instruction(),
            fault
        )
    }
}

/// 모드별 키워드. (모드, 우선순위 순서대로 나열 - 동점 tie-break에 사용)
/// 키워드는 공백 제거 없이 원문 substring 매칭.
fn mode_keywords() -> [(DebateMode, &'static [&'static str]); 7] {
    [
        (
            DebateMode::CasualBanter,
            &[
                "민트초코", "민초", "부먹", "찍먹", "탕수육", "호불호", "취향", "제맛",
                "디저트", "라면", "치킨", "짜장", "짬뽕", "피자", "에어컨", "보일러",
                "치약 맛", " vs ", "더 맛있", "간식",
            ],
        ),
        (
            DebateMode::Courtroom,
            &["판사", "재판", "법정", "유죄", "무죄", "판결", "배심", "공정"],
        ),
        (
            DebateMode::PolicyDuel,
            &[
                "규제", "법으로", "법률", "입법", "정책", "제도", "금지", "의무화",
                "오픈소스", "기본소득", "세금", "익명성", "폐지",
            ],
        ),
        (
            DebateMode::PersonalStakes,
            &["연애", "사랑", "친구", "가족", "아이", "스마트폰", "공동체", "데이트"],
        ),
        (
            DebateMode::MoralDilemma,
            &["윤리", "도덕", "존엄", "존중", "권리", "위로", "모독", "기억", "지워"],
        ),
        (
            DebateMode::Forecasting,
            &["예측", "전망", "미래에", "확률"],
        ),
        (
            DebateMode::DesignReview,
            &["설계", "디자인", "아키텍처", "시스템을"],
        ),
    ]
}

/// 키워드 점유로 모드를 고른다. 동점이면 `mode_keywords` 순서(구체→일반)가 이긴다.
/// 아무 키워드도 안 걸리면 기본 `MoralDilemma`(살롱 주제 대다수가 가치 트레이드오프).
fn infer_mode(joined: &str) -> DebateMode {
    let mut best = DebateMode::MoralDilemma;
    let mut best_score = 0usize;
    for (mode, keywords) in mode_keywords() {
        let score: usize = keywords.iter().map(|kw| joined.matches(kw).count()).sum();
        if score > best_score {
            best_score = score;
            best = mode;
        }
    }
    best
}

/// 모드별 개막 질문·stakes·대립축 템플릿(결정적). 주제 문자열을 끼워 넣는다.
fn mode_template(mode: DebateMode, topic: &str) -> (String, String, Vec<String>) {
    let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match mode {
        DebateMode::Courtroom => (
            format!("쟁점 '{topic}'을 법정처럼 다룹니다. 각자 주장마다 근거를 대고, 상대 주장의 약한 전제를 한 가지씩 교차신문하세요."),
            "잘못된 판단이 누구에게 어떤 피해를 주는가, 그리고 그 책임을 누가 지는가.".to_string(),
            v(&["편향 vs 일관성", "항소·구제 권리", "설명가능성", "책임 소재"]),
        ),
        DebateMode::PolicyDuel => (
            format!("'{topic}'를 정책 다툼으로 봅니다. 누가 무엇을 의무화/금지하고, 그 비용과 집행은 누가 지는지로 갈라서 논하세요."),
            "안전·공익과 혁신·자유 중 무엇을 누구의 비용으로 우선할 것인가.".to_string(),
            v(&["안전 vs 혁신", "자율 vs 강제", "책임 vs 공유재", "집행 가능성"]),
        ),
        DebateMode::MoralDilemma => (
            format!("'{topic}'에는 쉽게 화해되지 않는 가치 충돌이 있습니다. 각자 어떤 가치를 우선하는지 먼저 분명히 하고, 그 대가를 인정하며 논하세요."),
            "효율·욕구를 위해 어떤 권리·존엄을 어디까지 양보할 수 있는가.".to_string(),
            v(&["개인 권리 vs 공동선", "단기 위로 vs 장기 해악", "자율 vs 보호"]),
        ),
        DebateMode::PersonalStakes => (
            format!("'{topic}'를 추상 정책이 아니라 각자의 삶에서 시작해 논하세요. 구체적 경험 하나를 근거로 입장을 세우세요."),
            "이 선택이 실제 관계와 일상에서 누구를 더 외롭게/자유롭게 만드는가.".to_string(),
            v(&["친밀함 vs 편의", "선택의 자유 vs 책임", "개인 vs 관계"]),
        ),
        DebateMode::Forecasting => (
            format!("'{topic}'에 대해 각자 예측과 신뢰도를 내거세요. 무엇이 그 예측을 틀리게 할지도 함께 말하세요."),
            "어떤 미래가 더 그럴듯하며, 무엇을 보면 누가 틀렸는지 판가름 나는가.".to_string(),
            v(&["낙관 vs 비관", "속도 vs 한계", "측정 가능한 신호"]),
        ),
        DebateMode::DesignReview => (
            format!("'{topic}'를 설계 논쟁으로 다룹니다. 각자 한 가지 설계 대안을 들고, 트레이드오프와 실패 모드를 짚으세요."),
            "어떤 설계가 어떤 제약 아래 더 낫고, 무엇을 포기하게 되는가.".to_string(),
            v(&["단순함 vs 유연함", "비용 vs 견고함", "사용자 vs 운영"]),
        ),
        DebateMode::CasualBanter => (
            format!("'{topic}'! 진지할 거 없어, 반말로 가볍게 가자. 각자 취향 딱 정하고 웃긴 근거로 우겨봐."),
            "결론은 안 나도 돼 - 누가 더 재밌고 설득력 있게 우기느냐가 전부.".to_string(),
            v(&["내 취향 vs 네 취향", "경험담 vs 우김", "진지충 금지"]),
        ),
    }
}

/// 토픽들(최대 5개 권장)에서 DebatePlan을 결정적으로 추론한다.
pub fn infer_debate_plan(topics: &[String]) -> DebatePlan {
    let topic = topics
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let mode = infer_mode(&topic);
    let (opening_question, stakes, fault_lines) = mode_template(mode, &topic);
    DebatePlan {
        topic,
        mode,
        opening_question,
        stakes,
        fault_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_of(topic: &str) -> DebatePlan {
        infer_debate_plan(&[topic.to_string()])
    }

    #[test]
    fn ai_judge_topic_maps_to_courtroom() {
        let p = plan_of("AI 판사가 인간 판사보다 공정할 수 있을까?");
        assert_eq!(p.mode, DebateMode::Courtroom);
        assert!(p.opening_question.contains("AI 판사"));
        assert!(!p.fault_lines.is_empty());
    }

    #[test]
    fn open_source_regulation_maps_to_policy_duel() {
        let p = plan_of("AI 규제와 오픈소스");
        assert_eq!(p.mode, DebateMode::PolicyDuel);
    }

    #[test]
    fn romance_app_maps_to_personal_stakes() {
        let p = plan_of("연애 앱은 사랑을 돕는가, 소비하게 만드는가?");
        assert_eq!(p.mode, DebateMode::PersonalStakes);
    }

    #[test]
    fn moral_dilemma_default_and_keyword() {
        // 키워드 직격(기억/지워/권리)
        assert_eq!(
            plan_of("기억을 선택적으로 지울 수 있다면 지워도 될까?").mode,
            DebateMode::MoralDilemma
        );
        // 아무 키워드도 없는 주제 → 기본 MoralDilemma
        assert_eq!(plan_of("우주는 끝이 있을까").mode, DebateMode::MoralDilemma);
    }

    #[test]
    fn deterministic_and_multi_topic_join() {
        let topics = vec!["AI 규제와 오픈소스".to_string(), "보안 책임".to_string()];
        let a = infer_debate_plan(&topics);
        let b = infer_debate_plan(&topics);
        assert_eq!(a, b);
        assert_eq!(a.topic, "AI 규제와 오픈소스, 보안 책임");
    }

    #[test]
    fn fun_taste_topic_maps_to_casual_banter_with_banmal() {
        let p = plan_of("민트초코는 맛있는 디저트인가, 치약 맛 나는 음식인가?");
        assert_eq!(p.mode, DebateMode::CasualBanter);
        // 반말 지시가 directive에 들어가는지
        assert!(p.directive_line(0).contains("반말"));
    }

    #[test]
    fn directive_line_carries_mode_anchor_and_fault() {
        let p = plan_of("AI 판사가 인간 판사보다 공정할 수 있을까?"); // Courtroom
        let line = p.directive_line(0);
        assert!(line.starts_with("[토론]"));
        assert!(line.contains("법정형"));
        assert!(line.contains("대립축:"));
    }

    #[test]
    fn empty_topics_is_safe() {
        let p = infer_debate_plan(&[]);
        assert_eq!(p.topic, "");
        assert_eq!(p.mode, DebateMode::MoralDilemma);
    }
}
