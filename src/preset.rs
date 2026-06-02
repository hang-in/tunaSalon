use crate::model::{CouplingMatrix, EngineConfig, Persona};

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

    /// 프리셋 값으로 EngineConfig를 구성한다.
    ///
    /// alpha는 균일 off-diagonal 행렬:
    ///   α_base = target_rho * beta / (N - 1)   (N > 1)
    ///   대각 항목은 absent(0.0으로 취급).
    /// 이렇게 구성하면 분기 spectral radius == target_rho < 1 이므로 is_stable()이 항상 true.
    pub fn build_config(&self, personas: &[Persona]) -> EngineConfig {
        let (beta, theta, target_rho, _mu_scale) = self.params();
        let n = personas.len();

        let alpha = if n <= 1 {
            CouplingMatrix::new()
        } else {
            let alpha_base = target_rho * beta / (n as f64 - 1.0);
            let mut matrix = CouplingMatrix::new();
            for p in personas {
                for j in personas {
                    if p.id != j.id {
                        matrix.values.insert((p.id.clone(), j.id.clone()), alpha_base);
                    }
                }
            }
            matrix
        };

        EngineConfig {
            beta,
            theta,
            k: 60.0,
            tick_interval: 1.0,
            alpha,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hawkes::HawkesEngine;

    fn three_personas() -> Vec<Persona> {
        vec![
            Persona { id: "a".to_string(), name: "A".to_string(), base_rate: 0.8 },
            Persona { id: "b".to_string(), name: "B".to_string(), base_rate: 0.7 },
            Persona { id: "c".to_string(), name: "C".to_string(), base_rate: 0.25 },
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
            (RoomPreset::Calm,     0.10_f64),
            (RoomPreset::Pub,      0.40_f64),
            (RoomPreset::Argument, 0.80_f64),
            (RoomPreset::Chaos,    0.92_f64),
        ];
        for (preset, target_rho) in targets {
            let config = preset.build_config(&personas);
            let radius = HawkesEngine::branching_spectral_radius(
                &config.alpha,
                &personas,
                config.beta,
            );
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
}
