// 결정성: 직렬화/반복 순서가 실행마다 동일해야 하므로 BTreeMap이 아닌 BTreeMap(정렬 순서) 사용.
use std::collections::BTreeMap;

pub type PersonaId = String;

/// 설정 페이지에서 고를 수 있는 클라우드 모델 목록(ollama.com 확인, 2026-06).
/// backend 이름 = 모델 태그로 풀에 등록되고 PersonaMeta.backend로 라우팅·영속된다.
/// model.rs(비 feature-gate)에 두어 web/비-web 양쪽에서 공유한다.
pub const CLOUD_MODELS: [&str; 9] = [
    "gemma4:31b-cloud",
    "nemotron-3-super:cloud",
    "qwen3.5:cloud",
    "glm-5.1:cloud",
    "kimi-k2.6:cloud",
    "deepseek-v4-flash:cloud",
    "devstral-small-2:24b-cloud",
    // 고급(대형) — 토론 품질 테스트용. 태그는 ollama.com에서 사용자 직접 확인(2026-06-27).
    "minimax-m3:cloud",
    "deepseek-v4-pro:cloud",
];

/// 페르소나별 케미 비대칭 계수.
/// reactivity:    이 페르소나가 남의 발화에 얼마나 자극받나 (row 스케일).
/// provocativeness: 이 페르소나가 남을 얼마나 자극하나 (column 스케일).
/// 기본값은 둘 다 1.0 (균일, task-10 build_config와 동일).
#[derive(Debug, Clone, PartialEq)]
pub struct PersonaModifier {
    pub reactivity: f64,
    pub provocativeness: f64,
}

impl Default for PersonaModifier {
    fn default() -> Self {
        Self {
            reactivity: 1.0,
            provocativeness: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    pub beta: f64,
    pub theta: f64,
    pub k: f64,
    pub tick_interval: f64,
    pub alpha: CouplingMatrix,
    /// FSM 전이 제약: true면 같은 페르소나의 2연속 발화를 금지한다. 기본 false.
    pub forbid_self_repeat: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Persona {
    pub id: PersonaId,
    pub name: String,
    pub base_rate: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub ts: f64,
    pub speaker: PersonaId,
    pub mark: f64,
    pub content: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineState {
    pub intensities: BTreeMap<PersonaId, f64>,
    pub excitations: BTreeMap<PersonaId, f64>,
    pub history: Vec<Event>,
    pub last_speaker: Option<PersonaId>,
    pub rng_seed: u64,
}

// v0.2부터 사용
#[derive(Debug, Clone, PartialEq)]
pub struct CouplingMatrix {
    pub values: BTreeMap<(PersonaId, PersonaId), f64>,
}

impl CouplingMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, p: &PersonaId, j: &PersonaId) -> f64 {
        match self.values.get(&(p.clone(), j.clone())) {
            Some(value) => *value,
            None => 0.0,
        }
    }
}

impl Default for CouplingMatrix {
    fn default() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_core_model_types() {
        let config = EngineConfig {
            beta: 0.2,
            theta: 0.7,
            k: 60.0,
            tick_interval: 1.0,
            alpha: CouplingMatrix::default(),
            forbid_self_repeat: false,
        };
        let persona = Persona {
            id: "p1".to_string(),
            name: "Talker".to_string(),
            base_rate: 0.8,
        };
        let mut intensities = BTreeMap::new();
        intensities.insert(persona.id.clone(), persona.base_rate);
        let state = EngineState {
            intensities,
            excitations: BTreeMap::new(),
            history: Vec::new(),
            last_speaker: None,
            rng_seed: 42,
        };

        assert_eq!(config.tick_interval, 1.0);
        assert_eq!(persona.id, "p1");
        assert_eq!(state.intensities.get("p1"), Some(&0.8));
        assert_eq!(state.rng_seed, 42);
    }
}
