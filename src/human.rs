use crate::model::{EngineState, Event, Persona, PersonaId};

/// 사람 발화가 Hawkes 이벤트로서 엔진에 줄 자극 크기(큰 mark 값).
/// 페르소나 간 α 커플링과 독립적으로 history에 기록되는 외부 이벤트 강도.
/// 현재 동역학(combined_intensities)은 attention flat 자극을 사용하며,
/// mark는 observability 및 설계 충실도를 위해 큰 값으로 기록한다.
const HUMAN_MARK: f64 = 5.0;

/// 사람 발화 시 전 페르소나에 더할 기본 자극 크기.
/// 페르소나끼리의 α 계수(일반적으로 0.1~0.5)보다 크게 설정해
/// 사람이 말하면 모든 페르소나의 λ가 일제히 크게 차오르도록 한다.
const DEFAULT_ATTENTION: f64 = 1.5;

/// 사람 발화 시 기존 페르소나 간 excitation에 곱할 감쇠 계수.
/// 0.5 = "절반 리셋" — 페르소나끼리 쌓인 대화 모멘텀을 줄여
/// 사람에게 주목하도록 유도한다. 1.0이면 감쇠 없음, 0.0이면 완전 리셋.
const DEFAULT_RESET_FACTOR: f64 = 0.5;

/// 사람 참여 채널. 사람 발화를 엔진 상태에 반영하는 순수 엔진 로직.
///
/// 설계 §5 "사람 참여": 사람은 α 행렬에 없으므로 페르소나 간 커플링과 분리해
/// flat 강자극(attention)으로 모든 페르소나의 λ를 일제히 끌어올린다.
/// 기존 excitation을 reset_factor로 감쇠해 사람에게 시선이 집중되는 효과를 모델링.
///
/// rng 미소비, 비결정 요소 없음 → headless 골든 경로 불침투.
pub struct HumanChannel {
    speaker_id: PersonaId,
    attention: f64,
    reset_factor: f64,
}

impl HumanChannel {
    /// 기본 상수(attention=1.5, reset_factor=0.5)로 HumanChannel을 생성한다.
    pub fn new(speaker_id: impl Into<String>) -> Self {
        Self {
            speaker_id: speaker_id.into(),
            attention: DEFAULT_ATTENTION,
            reset_factor: DEFAULT_RESET_FACTOR,
        }
    }

    /// attention과 reset_factor를 명시적으로 지정해 생성한다.
    /// 라이브 튜닝(task-31) 또는 테스트에서 사용.
    pub fn with_params(speaker_id: impl Into<String>, attention: f64, reset_factor: f64) -> Self {
        Self {
            speaker_id: speaker_id.into(),
            attention,
            reset_factor,
        }
    }

    /// 사람이 발화했을 때 엔진 상태를 갱신한다.
    ///
    /// 1. **일부 리셋**: 기존 excitations에 reset_factor를 곱해
    ///    페르소나 간 대화 모멘텀을 줄인다(사람에게 주목하도록).
    /// 2. **강자극**: 각 페르소나에 attention을 더해
    ///    모든 페르소나의 λ가 일제히 상승하도록 한다.
    /// 3. **history push**: 사람 발화를 큰 mark의 Event로 기록한다.
    ///
    /// mark(HUMAN_MARK)는 observability·설계 충실도를 위해 큰 값으로 기록하지만
    /// 실제 동역학은 flat attention 자극에서 나온다. α 행렬과 무관하므로
    /// Hawkes 스펙트럼 반경(안정성 조건)에 영향을 주지 않는다.
    pub fn speak(&self, state: &mut EngineState, personas: &[Persona], text: String, ts: f64) {
        // 1. 일부 리셋: 페르소나 간 누적 excitation 감쇠 (주목 집중 효과)
        for excitation in state.excitations.values_mut() {
            *excitation *= self.reset_factor;
        }

        // 2. 강자극: 모든 페르소나에 flat하게 attention 추가
        for persona in personas {
            *state.excitations.entry(persona.id.clone()).or_insert(0.0) += self.attention;
        }

        // 3. history push: 사람 발화를 큰 mark의 외부 이벤트로 기록
        state.history.push(Event {
            ts,
            speaker: self.speaker_id.clone(),
            mark: HUMAN_MARK,
            content: Some(text),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hawkes::HawkesEngine;
    use std::collections::BTreeMap;

    fn personas() -> Vec<Persona> {
        vec![
            Persona {
                id: "aria".to_string(),
                name: "Aria".to_string(),
                base_rate: 0.4,
            },
            Persona {
                id: "bjorn".to_string(),
                name: "Bjorn".to_string(),
                base_rate: 0.7,
            },
        ]
    }

    fn empty_state() -> EngineState {
        EngineState {
            intensities: BTreeMap::from([("aria".to_string(), 0.4), ("bjorn".to_string(), 0.7)]),
            excitations: BTreeMap::new(),
            history: Vec::new(),
            last_speaker: None,
            rng_seed: 42,
        }
    }

    /// (1) speak 후 history 마지막 이벤트가 사람 Event인지 확인.
    /// speaker = speaker_id, mark = HUMAN_MARK, content = 입력 텍스트.
    #[test]
    fn speak_appends_human_event_to_history() {
        let channel = HumanChannel::new("you");
        let personas = personas();
        let mut state = empty_state();

        channel.speak(&mut state, &personas, "안녕하세요".to_string(), 10.0);

        let last = state
            .history
            .last()
            .expect("history가 비어있지 않아야 한다");
        assert_eq!(last.speaker, "you");
        assert_eq!(last.mark, HUMAN_MARK);
        assert_eq!(last.content, Some("안녕하세요".to_string()));
        assert_eq!(last.ts, 10.0);
    }

    /// (2) speak 후 모든 페르소나의 excitation이 호출 전보다 증가.
    /// combined_intensities를 직접 확인해 λ 상승도 검증.
    #[test]
    fn speak_raises_all_persona_excitations_and_combined_intensities() {
        let channel = HumanChannel::new("you");
        let personas = personas();
        let mut state = empty_state();

        // 발화 전 combined_intensities (excitation 없으므로 base와 동일)
        let before =
            HawkesEngine::combined_intensities(&state.intensities, &state.excitations, &personas);

        channel.speak(&mut state, &personas, "반가워요".to_string(), 5.0);

        // 발화 후 excitation이 모든 페르소나에 대해 양수여야 한다
        for persona in &personas {
            let exc = state.excitations.get(&persona.id).copied().unwrap_or(0.0);
            assert!(
                exc > 0.0,
                "페르소나 {} excitation이 증가해야 한다",
                persona.id
            );
        }

        // combined_intensities도 모두 상승해야 한다
        let after =
            HawkesEngine::combined_intensities(&state.intensities, &state.excitations, &personas);
        for persona in &personas {
            let b = before.get(&persona.id).copied().unwrap_or(0.0);
            let a = after.get(&persona.id).copied().unwrap_or(0.0);
            assert!(
                a > b,
                "페르소나 {} combined intensity가 상승해야 한다",
                persona.id
            );
        }
    }

    /// (3) 기존 excitation이 있을 때 reset_factor 감쇠 후 attention 추가.
    /// 예: 기존 2.0, reset 0.5, attention 1.5 → 2.0*0.5 + 1.5 = 2.5 (정확한 산술).
    #[test]
    fn speak_applies_reset_then_attention_to_existing_excitation() {
        let channel = HumanChannel::with_params("you", 1.5, 0.5);
        let personas = personas();
        let mut state = empty_state();

        // aria에만 기존 excitation 2.0 설정
        state.excitations.insert("aria".to_string(), 2.0);

        channel.speak(&mut state, &personas, "테스트".to_string(), 0.0);

        let aria_exc = state.excitations.get("aria").copied().unwrap_or(0.0);
        // 2.0 * 0.5 + 1.5 = 2.5
        assert!(
            (aria_exc - 2.5).abs() < 1e-12,
            "aria excitation은 2.5여야 한다. 실제: {aria_exc}"
        );

        let bjorn_exc = state.excitations.get("bjorn").copied().unwrap_or(0.0);
        // bjorn는 기존 excitation 없음: 0.0 * 0.5 + 1.5 = 1.5
        assert!(
            (bjorn_exc - 1.5).abs() < 1e-12,
            "bjorn excitation은 1.5여야 한다. 실제: {bjorn_exc}"
        );
    }

    /// (4) 동일 입력 두 번 호출이 결정적: 같은 시작 상태에서 speak 결과가 동일.
    #[test]
    fn speak_is_deterministic_for_same_inputs() {
        let personas = personas();

        let mut state_a = empty_state();
        let channel = HumanChannel::with_params("you", 1.5, 0.5);
        channel.speak(&mut state_a, &personas, "hello".to_string(), 7.0);

        let mut state_b = empty_state();
        let channel2 = HumanChannel::with_params("you", 1.5, 0.5);
        channel2.speak(&mut state_b, &personas, "hello".to_string(), 7.0);

        assert_eq!(state_a.excitations, state_b.excitations);
        assert_eq!(state_a.history, state_b.history);
    }

    /// (5) alpha matrix 없이도 combined_intensities 계산과 독립적임을 확인.
    /// HumanChannel은 CouplingMatrix를 전혀 참조하지 않는다.
    #[test]
    fn speak_does_not_depend_on_coupling_matrix() {
        let personas = personas();
        let mut state = empty_state();
        let channel = HumanChannel::new("you");

        // CouplingMatrix 없이도 정상 동작
        channel.speak(&mut state, &personas, "독립성 테스트".to_string(), 1.0);

        // 두 페르소나 모두 attention 만큼 excitation 증가
        let aria_exc = state.excitations.get("aria").copied().unwrap_or(0.0);
        let bjorn_exc = state.excitations.get("bjorn").copied().unwrap_or(0.0);
        assert!((aria_exc - DEFAULT_ATTENTION).abs() < 1e-12);
        assert!((bjorn_exc - DEFAULT_ATTENTION).abs() < 1e-12);
    }

    /// (6) 상수값 노출: HUMAN_MARK, DEFAULT_ATTENTION, DEFAULT_RESET_FACTOR가
    /// 설계 문서의 기대값과 일치하는지 회귀 보호.
    #[test]
    fn constants_match_design_spec() {
        assert_eq!(HUMAN_MARK, 5.0);
        assert_eq!(DEFAULT_ATTENTION, 1.5);
        assert_eq!(DEFAULT_RESET_FACTOR, 0.5);
    }
}
