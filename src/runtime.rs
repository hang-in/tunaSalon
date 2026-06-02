use crate::model::{Event, PersonaId};
use rand_chacha::ChaCha8Rng;

/// 화자가 확정된 뒤 실제 발화 텍스트를 생성하는 계약(trait).
///
/// - `Some(String)` 반환 시 해당 텍스트가 Event.content / record.utterance에 기록된다.
/// - `None` 반환 시 Event.content / record.utterance는 None으로 남아 JSON에서 생략된다.
///
/// 구현체는 rng를 소비해도 되지만, FakeBackend는 rng를 건드리지 않는다.
/// 이로써 기본 실행(FakeBackend)에서 v0.2 rng 소비 순서가 변하지 않아
/// 골든 결과가 바이트 동일하게 보존된다.
pub trait PersonaRuntime {
    fn generate(
        &mut self,
        speaker: &PersonaId,
        history: &[Event],
        tick: u64,
        rng: &mut ChaCha8Rng,
    ) -> Option<String>;
}

/// 결정적 stub 백엔드. 항상 `None`을 반환한다(내용 없음 = v0.1/v0.2 동작).
///
/// rng를 전혀 소비하지 않으므로 driver의 기존 rng 소비 순서(rrf::select, make_utterance)가
/// 그대로 유지되어 v0.2 골든 5종이 바이트 동일하게 보존된다.
/// LLM 호출 없이 결정적으로 동작해야 하는 기본 실행(`cargo run`) 및 단위/통합 테스트에서 사용한다.
pub struct FakeBackend;

impl PersonaRuntime for FakeBackend {
    fn generate(
        &mut self,
        _speaker: &PersonaId,
        _history: &[Event],
        _tick: u64,
        _rng: &mut ChaCha8Rng,
    ) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn fake_backend_generate_always_returns_none() {
        let mut backend = FakeBackend;
        let speaker = "friend".to_string();
        let history = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        let result = backend.generate(&speaker, &history, 0, &mut rng);
        assert_eq!(result, None);
    }

    #[test]
    fn fake_backend_is_object_safe_and_does_not_panic() {
        // PersonaRuntime이 dyn으로 쓰일 수 있어야 task-16의 OllamaBackend가 같은 트레이트로 들어올 수 있다.
        let mut backend = FakeBackend;
        let runtime: &mut dyn PersonaRuntime = &mut backend;

        let speaker = "chaos".to_string();
        let history = vec![
            Event { ts: 0.0, speaker: "friend".to_string(), mark: 1.0, content: None },
            Event { ts: 1.0, speaker: "chaos".to_string(), mark: 1.0, content: None },
        ];
        let mut rng = ChaCha8Rng::seed_from_u64(99);

        let result = runtime.generate(&speaker, &history, 5, &mut rng);
        assert_eq!(result, None);
    }
}
