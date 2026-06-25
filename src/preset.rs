use crate::hawkes::HawkesEngine;
use crate::model::{CouplingMatrix, EngineConfig, Persona, PersonaId, PersonaModifier};
use std::collections::BTreeMap;

/// persona 목록과 모디파이어로 coupling α 행렬을 계산한다.
///
/// `build_config_with_modifiers`의 alpha 계산 블록을 free 함수로 추출.
/// 동적 초대(`add_persona`/`remove_persona`) 시에도 동일 로직으로 재계산하는 데 사용.
///
/// - n <= 1이면 빈 행렬 반환.
/// - raw = reactivity(p) * provocativeness(j), p != j.
/// - branching_spectral_radius로 재정규화(target_rho).
/// - cur <= 0이면 빈 행렬(자극 없음).
pub fn coupling_from_modifiers(
    personas: &[Persona],
    modifiers: &BTreeMap<PersonaId, PersonaModifier>,
    beta: f64,
    target_rho: f64,
) -> CouplingMatrix {
    let n = personas.len();
    if n <= 1 || target_rho <= 0.0 {
        return CouplingMatrix::new();
    }

    // 1단계: raw 비대칭 α 계산
    let mut raw = CouplingMatrix::new();
    for p in personas {
        let reactivity = modifiers
            .get(&p.id)
            .map(|m| m.reactivity)
            .unwrap_or(PersonaModifier::default().reactivity);
        for j in personas {
            if p.id != j.id {
                let provocativeness = modifiers
                    .get(&j.id)
                    .map(|m| m.provocativeness)
                    .unwrap_or(PersonaModifier::default().provocativeness);
                let value = reactivity * provocativeness;
                if value > 0.0 {
                    raw.values.insert((p.id.clone(), j.id.clone()), value);
                }
            }
        }
    }

    // 2단계: 분기 spectral radius 계산 후 재정규화
    let cur = HawkesEngine::branching_spectral_radius(&raw, personas, beta);
    if cur <= 0.0 {
        return CouplingMatrix::new();
    }
    let scale = target_rho / cur;
    let mut scaled = CouplingMatrix::new();
    for (key, value) in &raw.values {
        scaled.values.insert(key.clone(), value * scale);
    }
    scaled
}

/// 방 분위기 프리셋. 각 프리셋은 β/θ/α를 한 번에 세팅한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomPreset {
    Calm,
    Pub,
    Argument,
    Chaos,
}

impl RoomPreset {
    /// 대소문자 구분 없이 "calm"/"pub"/"argument"/"chaos" 파싱.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "calm" => Ok(RoomPreset::Calm),
            "pub" => Ok(RoomPreset::Pub),
            "argument" => Ok(RoomPreset::Argument),
            "chaos" => Ok(RoomPreset::Chaos),
            other => Err(format!(
                "unknown room preset: \"{other}\". Valid values: calm, pub, argument, chaos"
            )),
        }
    }

    /// (beta, theta, target_rho, mu_scale) 반환.
    fn params(&self) -> (f64, f64, f64, f64) {
        match self {
            RoomPreset::Calm => (0.8, 0.70, 0.10, 0.8),
            RoomPreset::Pub => (0.5, 0.50, 0.40, 1.0),
            RoomPreset::Argument => (0.3, 0.38, 0.80, 1.0),
            RoomPreset::Chaos => (0.3, 0.35, 0.92, 1.0),
        }
    }

    /// 페르소나 base_rate에 곱할 스케일 팩터.
    pub fn mu_scale(&self) -> f64 {
        self.params().3
    }

    /// alpha 정규화 목표 spectral radius.
    pub fn target_rho(&self) -> f64 {
        self.params().2
    }

    /// 프리셋 값으로 EngineConfig를 구성한다.
    ///
    /// 모디파이어를 전부 기본(1.0)으로 설정한 build_config_with_modifiers와 동일.
    /// alpha는 균일 off-diagonal 행렬, 분기 spectral radius == target_rho < 1.
    pub fn build_config(&self, personas: &[Persona]) -> EngineConfig {
        self.build_config_with_modifiers(personas, &BTreeMap::new())
    }

    /// 페르소나별 모디파이어를 반영한 비대칭 α 행렬로 EngineConfig를 구성한다.
    ///
    /// 비대칭 raw α:
    ///   raw_pj = reactivity(p) * provocativeness(j)   (p != j, 대각 absent)
    ///   모디파이어가 없는 페르소나는 기본값(1.0)으로 취급.
    ///
    /// 안정 재정규화:
    ///   cur = branching_spectral_radius(raw_alpha, personas, beta)
    ///   cur > 0 이면 모든 항목에 (target_rho / cur) 를 곱해 spectral radius == target_rho.
    ///   cur <= 0 이면 빈 행렬 반환(자극 없음).
    pub fn build_config_with_modifiers(
        &self,
        personas: &[Persona],
        modifiers: &BTreeMap<PersonaId, PersonaModifier>,
    ) -> EngineConfig {
        let (beta, theta, target_rho, _mu_scale) = self.params();
        let alpha = coupling_from_modifiers(personas, modifiers, beta, target_rho);

        EngineConfig {
            beta,
            theta,
            k: 60.0,
            tick_interval: 1.0,
            alpha,
            forbid_self_repeat: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hawkes::HawkesEngine;

    fn three_personas() -> Vec<Persona> {
        vec![
            Persona {
                id: "a".to_string(),
                name: "A".to_string(),
                base_rate: 0.8,
            },
            Persona {
                id: "b".to_string(),
                name: "B".to_string(),
                base_rate: 0.7,
            },
            Persona {
                id: "c".to_string(),
                name: "C".to_string(),
                base_rate: 0.25,
            },
        ]
    }

    const ALL_PRESETS: &[RoomPreset] = &[
        RoomPreset::Calm,
        RoomPreset::Pub,
        RoomPreset::Argument,
        RoomPreset::Chaos,
    ];

    // 테스트 1: 네 preset 모두 is_stable == true (3-persona 기준)
    #[test]
    fn all_presets_are_stable_with_three_personas() {
        let personas = three_personas();
        for preset in ALL_PRESETS {
            let config = preset.build_config(&personas);
            assert!(
                HawkesEngine::is_stable(&config.alpha, &personas, config.beta),
                "{preset:?} is not stable"
            );
        }
    }

    // 테스트 2: branching_spectral_radius가 각 preset의 target_rho와 1e-6 이내로 일치
    #[test]
    fn spectral_radius_matches_target_rho_within_tolerance() {
        let personas = three_personas();
        let targets = [
            (RoomPreset::Calm, 0.10_f64),
            (RoomPreset::Pub, 0.40_f64),
            (RoomPreset::Argument, 0.80_f64),
            (RoomPreset::Chaos, 0.92_f64),
        ];
        for (preset, target_rho) in targets {
            let config = preset.build_config(&personas);
            let radius =
                HawkesEngine::branching_spectral_radius(&config.alpha, &personas, config.beta);
            assert!(
                (radius - target_rho).abs() < 1e-6,
                "{preset:?}: radius={radius} expected={target_rho} diff={}",
                (radius - target_rho).abs()
            );
        }
    }

    // 테스트 3: α 행렬이 균일 off-diagonal·대각 0
    #[test]
    fn alpha_matrix_is_uniform_off_diagonal_with_zero_diagonal() {
        let personas = three_personas();
        for preset in ALL_PRESETS {
            let config = preset.build_config(&personas);
            let alpha = &config.alpha;

            // 대각선 항목은 맵에 없어야 한다 (get() == 0.0).
            for p in &personas {
                assert_eq!(
                    alpha.get(&p.id, &p.id),
                    0.0,
                    "{preset:?}: diagonal [{id}][{id}] should be 0",
                    id = p.id
                );
            }

            // off-diagonal은 전부 동일한 양수여야 한다.
            let mut off_values: Vec<f64> = Vec::new();
            for p in &personas {
                for j in &personas {
                    if p.id != j.id {
                        off_values.push(alpha.get(&p.id, &j.id));
                    }
                }
            }
            let first = off_values[0];
            assert!(first > 0.0, "{preset:?}: off-diagonal should be positive");
            for v in &off_values {
                assert!(
                    (v - first).abs() < 1e-15,
                    "{preset:?}: off-diagonal values are not uniform: {v} vs {first}"
                );
            }
        }
    }

    // 테스트 4: Argument의 θ < Calm의 θ, Argument의 target_rho > Calm의 target_rho
    #[test]
    fn argument_has_lower_theta_and_higher_rho_than_calm() {
        let personas = three_personas();
        let calm_config = RoomPreset::Calm.build_config(&personas);
        let argument_config = RoomPreset::Argument.build_config(&personas);

        assert!(
            argument_config.theta < calm_config.theta,
            "Argument.theta ({}) should be < Calm.theta ({})",
            argument_config.theta,
            calm_config.theta
        );

        let calm_rho = HawkesEngine::branching_spectral_radius(
            &calm_config.alpha,
            &personas,
            calm_config.beta,
        );
        let argument_rho = HawkesEngine::branching_spectral_radius(
            &argument_config.alpha,
            &personas,
            argument_config.beta,
        );
        assert!(
            argument_rho > calm_rho,
            "Argument.rho ({argument_rho}) should be > Calm.rho ({calm_rho})"
        );
    }

    // parse() 동작 검증 (보너스)
    #[test]
    fn parse_valid_and_invalid_presets() {
        assert_eq!(RoomPreset::parse("calm").unwrap(), RoomPreset::Calm);
        assert_eq!(RoomPreset::parse("Pub").unwrap(), RoomPreset::Pub);
        assert_eq!(RoomPreset::parse("ARGUMENT").unwrap(), RoomPreset::Argument);
        assert_eq!(RoomPreset::parse("chaos").unwrap(), RoomPreset::Chaos);
        assert!(RoomPreset::parse("jazz").is_err());
    }

    // ── task-11 신규 테스트 ──────────────────────────────────────────────────

    /// 대비되는 모디파이어를 넣으면 α 행렬이 비대칭 (α_pj != α_jp 인 쌍이 존재).
    #[test]
    fn contrasting_modifiers_produce_asymmetric_alpha() {
        let personas = three_personas();
        // a: 높은 도발성, b: 높은 반응성, c: 둘 다 낮음
        let mut modifiers = BTreeMap::new();
        modifiers.insert(
            "a".to_string(),
            PersonaModifier {
                reactivity: 0.6,
                provocativeness: 2.0,
            },
        );
        modifiers.insert(
            "b".to_string(),
            PersonaModifier {
                reactivity: 2.0,
                provocativeness: 1.0,
            },
        );
        modifiers.insert(
            "c".to_string(),
            PersonaModifier {
                reactivity: 0.5,
                provocativeness: 0.5,
            },
        );

        let config = RoomPreset::Pub.build_config_with_modifiers(&personas, &modifiers);
        let alpha = &config.alpha;

        // b→a: reactivity(b)*provocativeness(a) raw = 2.0*2.0 = 4.0
        // a→b: reactivity(a)*provocativeness(b) raw = 0.6*1.0 = 0.6
        // raw가 다르므로 scaled도 다르다.
        let ab = alpha.get(&"a".to_string(), &"b".to_string());
        let ba = alpha.get(&"b".to_string(), &"a".to_string());
        assert!(
            (ab - ba).abs() > 1e-9,
            "expected asymmetry: α_ab={ab} α_ba={ba}"
        );
    }

    /// 비대칭 α 도 is_stable 이고 branching_spectral_radius ≈ target_rho (tol 1e-6).
    #[test]
    fn asymmetric_config_is_stable_and_matches_target_rho() {
        let personas = three_personas();
        let mut modifiers = BTreeMap::new();
        modifiers.insert(
            "a".to_string(),
            PersonaModifier {
                reactivity: 0.6,
                provocativeness: 2.0,
            },
        );
        modifiers.insert(
            "b".to_string(),
            PersonaModifier {
                reactivity: 2.0,
                provocativeness: 1.0,
            },
        );
        modifiers.insert(
            "c".to_string(),
            PersonaModifier {
                reactivity: 0.5,
                provocativeness: 0.5,
            },
        );

        // Pub preset의 target_rho = 0.40
        let target_rho = 0.40_f64;
        let config = RoomPreset::Pub.build_config_with_modifiers(&personas, &modifiers);

        assert!(
            HawkesEngine::is_stable(&config.alpha, &personas, config.beta),
            "asymmetric config should be stable"
        );

        let radius = HawkesEngine::branching_spectral_radius(&config.alpha, &personas, config.beta);
        assert!(
            (radius - target_rho).abs() < 1e-6,
            "asymmetric radius={radius} expected={target_rho} diff={}",
            (radius - target_rho).abs()
        );
    }

    /// 모디파이어가 전부 기본(빈 맵)이면 build_config_with_modifiers == build_config (균일 행렬).
    #[test]
    fn empty_modifiers_equals_build_config() {
        let personas = three_personas();
        for preset in ALL_PRESETS {
            let uniform = preset.build_config(&personas);
            let from_modifiers = preset.build_config_with_modifiers(&personas, &BTreeMap::new());

            assert_eq!(
                uniform.beta, from_modifiers.beta,
                "{preset:?}: beta mismatch"
            );
            assert_eq!(
                uniform.theta, from_modifiers.theta,
                "{preset:?}: theta mismatch"
            );
            // α 모든 항목 일치 확인
            for p in &personas {
                for j in &personas {
                    let v1 = uniform.alpha.get(&p.id, &j.id);
                    let v2 = from_modifiers.alpha.get(&p.id, &j.id);
                    assert!(
                        (v1 - v2).abs() < 1e-12,
                        "{preset:?}: alpha[{}][{}] uniform={v1} modifiers={v2}",
                        p.id,
                        j.id
                    );
                }
            }
        }
    }
}
