//! 단계형 토론 상태머신. 순수·결정적. live/web 비의존(framework-independent core).
//!
//! 토론을 오프닝 → 입장개진 → 공방 → 클로징 → 종료로 진행한다. 단계 전환은
//! **발화 수(주) + 수렴 신호(보조)**: `(발화수 ≥ 쿼터) OR (수렴 high AND 발화수 ≥ 최소)`.
//! 수렴↑ = 같은 말 반복(할 말 떨어짐) → 조기 전환. 발산(의견 안 좁혀짐)은 정상 → 쿼터까지 진행.
//!
//! rng·IO 없음. `LiveSession`은 `debate_plan`이 Some일 때만 이걸 구동하므로
//! driver/headless(골든) 경로는 전혀 영향받지 않는다.

use super::plan::DebateMode;

/// 수렴 조기전환 임계값. live.rs `CONVERGENCE_TWIST_THRESHOLD`와 같은 값(0.6).
const CONVERGENCE_HIGH: f64 = 0.6;

/// 토론 단계.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebatePhase {
    /// 사회자처럼 쟁점 개막 + 첫 입장 유도.
    Opening,
    /// 각자 입장 1번씩 분명히.
    Positions,
    /// 닉네임 부르며 동의/반박(메인 국면).
    Clash,
    /// 새 논거 금지, 각자 최종 입장 정리(정리자 우선).
    Closing,
    /// 종료. dispatch 중단(방 idle).
    Concluded,
}

/// 단계 진행을 추적하는 컨트롤러. `debate_plan`이 있을 때만 LiveSession이 구동한다.
#[derive(Debug, Clone)]
pub struct PhaseController {
    pub phase: DebatePhase,
    /// 현재 단계에서 누적된 (실)발화 수.
    utterances_in_phase: u32,
    mode: DebateMode,
    persona_count: u32,
}

impl PhaseController {
    /// 모드·인원으로 초기화. 시작 단계는 `Opening`.
    pub fn new(mode: DebateMode, persona_count: u32) -> Self {
        Self {
            phase: DebatePhase::Opening,
            utterances_in_phase: 0,
            mode,
            persona_count: persona_count.max(1),
        }
    }

    pub fn is_concluded(&self) -> bool {
        self.phase == DebatePhase::Concluded
    }

    /// persona 수 변동(동적 초대/퇴장) 반영. 다음 `bounds` 계산에 쓰인다.
    pub fn set_persona_count(&mut self, n: u32) {
        self.persona_count = n.max(1);
    }

    /// 한 발화가 디스패치된 뒤 호출. `convergence`(content 없으면 None)를 받아 전환 판정.
    /// `Concluded`에 막 도달한 순간에만 true 반환(호출자가 "토론 종료" 1회 알림용).
    pub fn on_utterance(&mut self, convergence: Option<f64>) -> bool {
        if self.phase == DebatePhase::Concluded {
            return false;
        }
        self.utterances_in_phase += 1;
        let (min, quota) = self.bounds();
        let conv_high = convergence.is_some_and(|c| c > CONVERGENCE_HIGH);
        let advance =
            self.utterances_in_phase >= quota || (conv_high && self.utterances_in_phase >= min);
        if advance {
            self.phase = self.next_phase();
            self.utterances_in_phase = 0;
            return self.phase == DebatePhase::Concluded;
        }
        false
    }

    fn next_phase(&self) -> DebatePhase {
        match self.phase {
            DebatePhase::Opening => DebatePhase::Positions,
            DebatePhase::Positions => DebatePhase::Clash,
            DebatePhase::Clash => DebatePhase::Closing,
            DebatePhase::Closing => DebatePhase::Concluded,
            DebatePhase::Concluded => DebatePhase::Concluded,
        }
    }

    /// (최소, 쿼터). persona 수·모드 반영.
    fn bounds(&self) -> (u32, u32) {
        let n = self.persona_count;
        match self.phase {
            DebatePhase::Opening => (1, 1),
            DebatePhase::Positions => (n, n),
            DebatePhase::Clash => {
                let rounds = match self.mode {
                    DebateMode::CasualBanter => 2,
                    DebateMode::Courtroom | DebateMode::PolicyDuel => 4,
                    _ => 3,
                };
                (n, rounds * n)
            }
            DebatePhase::Closing => {
                let q = if self.mode == DebateMode::CasualBanter { 1 } else { 2 };
                (1, q)
            }
            DebatePhase::Concluded => (0, 0),
        }
    }

    /// 종료된 방에 사람이 발화하면 공방으로 재진입한다.
    pub fn reopen_to_clash(&mut self) {
        self.phase = DebatePhase::Clash;
        self.utterances_in_phase = 0;
    }

    /// 생성 워커에 주입할 단계 지시 한 줄. `Concluded`면 빈 문자열.
    pub fn directive(&self) -> &'static str {
        match self.phase {
            DebatePhase::Opening =>
                "[단계: 오프닝] 사회자처럼 쟁점을 한 문장으로 열고, 당신의 첫 입장을 짧게 선언하세요. 아직 반박은 하지 마세요.",
            DebatePhase::Positions =>
                "[단계: 입장개진] 다른 사람을 반박하기 전에, 먼저 당신의 입장과 핵심 근거 하나를 분명히 세우세요.",
            DebatePhase::Clash =>
                "[단계: 공방] 최근 발언자 닉네임을 직접 부르며 동의/반박/보완 중 하나를 분명히 하고, 새 근거나 사례를 보태세요.",
            DebatePhase::Closing =>
                "[단계: 클로징] 새 논거를 꺼내지 말고, 지금까지의 논의를 받아 당신의 최종 입장을 한두 문장으로 정리하세요. 합의가 안 됐다면 무엇이 끝까지 갈렸는지 짚으세요.",
            DebatePhase::Concluded => "",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Opening은 1발화로 Positions로 전진한다.
    #[test]
    fn opening_advances_after_one() {
        let mut pc = PhaseController::new(DebateMode::MoralDilemma, 3);
        assert_eq!(pc.phase, DebatePhase::Opening);
        assert!(!pc.on_utterance(None));
        assert_eq!(pc.phase, DebatePhase::Positions);
    }

    /// Positions는 N(=3)발화로 Clash로 전진한다.
    #[test]
    fn positions_advances_after_n() {
        let mut pc = PhaseController::new(DebateMode::MoralDilemma, 3);
        pc.on_utterance(None); // Opening -> Positions
        pc.on_utterance(None); // 1
        pc.on_utterance(None); // 2
        assert_eq!(pc.phase, DebatePhase::Positions);
        pc.on_utterance(None); // 3 -> Clash
        assert_eq!(pc.phase, DebatePhase::Clash);
    }

    /// Clash: 수렴 high면 최소(N) 이후 조기 전환.
    #[test]
    fn clash_early_advances_on_high_convergence() {
        let mut pc = PhaseController::new(DebateMode::MoralDilemma, 3);
        // Opening -> Positions -> Clash
        pc.on_utterance(None);
        for _ in 0..3 {
            pc.on_utterance(None);
        }
        assert_eq!(pc.phase, DebatePhase::Clash);
        // 최소 N=3 발화 동안은 수렴 high여도 유지
        pc.on_utterance(Some(0.9)); // 1
        pc.on_utterance(Some(0.9)); // 2
        assert_eq!(pc.phase, DebatePhase::Clash);
        pc.on_utterance(Some(0.9)); // 3 == min → 조기 Closing
        assert_eq!(pc.phase, DebatePhase::Closing);
    }

    /// Clash: 수렴 low면 쿼터(3N=9)까지 유지한 뒤 전환.
    #[test]
    fn clash_holds_until_quota_when_low_convergence() {
        let mut pc = PhaseController::new(DebateMode::MoralDilemma, 3);
        pc.on_utterance(None);
        for _ in 0..3 {
            pc.on_utterance(None);
        }
        assert_eq!(pc.phase, DebatePhase::Clash);
        for _ in 0..8 {
            assert!(!pc.on_utterance(Some(0.1)));
        }
        assert_eq!(pc.phase, DebatePhase::Clash); // 8발화, 쿼터 9 미달
        pc.on_utterance(Some(0.1)); // 9 → Closing
        assert_eq!(pc.phase, DebatePhase::Closing);
    }

    /// Closing 쿼터를 채우면 Concluded + on_utterance가 true를 1회 반환.
    #[test]
    fn closing_concludes_and_signals_once() {
        let mut pc = PhaseController::new(DebateMode::MoralDilemma, 1);
        // 1인 방: Opening(1) -> Positions(1) -> Clash(rounds*1=3) -> Closing(2)
        assert!(!pc.on_utterance(None)); // Opening->Positions
        assert!(!pc.on_utterance(None)); // Positions->Clash
        for _ in 0..3 {
            pc.on_utterance(None); // Clash 채움 -> Closing
        }
        assert_eq!(pc.phase, DebatePhase::Closing);
        assert!(!pc.on_utterance(None)); // Closing 1
        assert!(pc.on_utterance(None)); // Closing 2 -> Concluded (true 1회)
        assert_eq!(pc.phase, DebatePhase::Concluded);
    }

    /// Concluded에서 on_utterance는 항상 false(추가 증가 없음).
    #[test]
    fn concluded_is_terminal() {
        let mut pc = PhaseController::new(DebateMode::CasualBanter, 2);
        pc.phase = DebatePhase::Concluded;
        assert!(!pc.on_utterance(None));
        assert!(!pc.on_utterance(Some(0.9)));
        assert_eq!(pc.phase, DebatePhase::Concluded);
    }

    /// reopen_to_clash 후 다시 진행 가능.
    #[test]
    fn reopen_resumes_at_clash() {
        let mut pc = PhaseController::new(DebateMode::MoralDilemma, 3);
        pc.phase = DebatePhase::Concluded;
        pc.reopen_to_clash();
        assert_eq!(pc.phase, DebatePhase::Clash);
        // 새로 9발화(3N)까지 유지
        for _ in 0..8 {
            pc.on_utterance(Some(0.1));
        }
        assert_eq!(pc.phase, DebatePhase::Clash);
    }

    /// 모드별 Clash 쿼터 손계산: CasualBanter=2N, Courtroom=4N.
    #[test]
    fn mode_specific_clash_bounds() {
        let casual = PhaseController {
            phase: DebatePhase::Clash,
            utterances_in_phase: 0,
            mode: DebateMode::CasualBanter,
            persona_count: 3,
        };
        assert_eq!(casual.bounds(), (3, 6)); // 2*3

        let court = PhaseController {
            phase: DebatePhase::Clash,
            utterances_in_phase: 0,
            mode: DebateMode::Courtroom,
            persona_count: 3,
        };
        assert_eq!(court.bounds(), (3, 12)); // 4*3

        let moral = PhaseController {
            phase: DebatePhase::Clash,
            utterances_in_phase: 0,
            mode: DebateMode::MoralDilemma,
            persona_count: 3,
        };
        assert_eq!(moral.bounds(), (3, 9)); // 3*3
    }

    /// set_persona_count가 다음 bounds에 반영된다.
    #[test]
    fn persona_count_change_affects_bounds() {
        let mut pc = PhaseController::new(DebateMode::MoralDilemma, 3);
        pc.phase = DebatePhase::Positions;
        assert_eq!(pc.bounds(), (3, 3));
        pc.set_persona_count(5);
        assert_eq!(pc.bounds(), (5, 5));
    }
}
