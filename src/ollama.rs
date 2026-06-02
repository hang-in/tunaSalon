use crate::model::{Event, PersonaId};
use crate::runtime::PersonaRuntime;
use rand_chacha::ChaCha8Rng;
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

/// Ollama HTTP 백엔드. 실제 LLM에 generate 요청을 POST한다.
///
/// SECURITY: `#[derive(Debug)]`를 쓰면 api_key가 노출되므로 수동 구현.
/// api_key는 Authorization 헤더에만 사용하며 로그/에러/Debug 출력에 절대 포함하지 않는다.
pub struct OllamaBackend {
    client: reqwest::blocking::Client,
    model: String,
    endpoint: String,
    api_key: Option<String>,
    system_prompts: BTreeMap<PersonaId, String>,
}

/// SECURITY: api_key를 절대 출력하지 않는다. Some/None 여부만 표시한다.
/// system_prompts는 민감 정보가 아니므로 표시해도 된다.
impl std::fmt::Debug for OllamaBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OllamaBackend")
            .field("model", &self.model)
            .field("endpoint", &self.endpoint)
            .field(
                "api_key",
                &self.api_key.as_ref().map(|_| "<redacted>"),
            )
            .field("system_prompts", &self.system_prompts)
            .finish()
    }
}

impl OllamaBackend {
    /// 새 OllamaBackend를 생성한다.
    ///
    /// - `model`: 사용할 Ollama 모델 이름 (예: "gemma4:e4b")
    /// - `endpoint`: Ollama 서버 주소 (예: "http://localhost:11434")
    /// - `api_key`: Ollama Cloud 인증 키. None이면 Authorization 헤더를 붙이지 않는다.
    /// - `system_prompts`: 화자별 system prompt 맵. PersonaId → 역할 지시문.
    /// - `timeout`: HTTP 요청 타임아웃
    ///
    /// reqwest Client 빌드에 실패하면 기본 Client로 폴백한다(panic 없음).
    pub fn new(
        model: String,
        endpoint: String,
        api_key: Option<String>,
        system_prompts: BTreeMap<PersonaId, String>,
        timeout: Duration,
    ) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self { client, model, endpoint, api_key, system_prompts }
    }

    /// `/api/generate` 요청 body JSON을 조립한다.
    ///
    /// - `system`이 Some이면 body에 `"system"` 필드를 추가한다.
    /// - None이면 `"system"` 필드를 완전히 생략한다.
    ///
    /// 별도 함수로 분리해 테스트에서 네트워크 없이 직렬화를 검증한다.
    pub fn build_request_body(model: &str, prompt: &str, system: Option<&str>) -> Value {
        let mut body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false
        });
        if let Some(sys) = system {
            body["system"] = serde_json::Value::String(sys.to_string());
        }
        body
    }

    /// 응답 JSON에서 "response" 필드를 추출한다.
    ///
    /// 별도 함수로 분리해 테스트에서 네트워크 없이 파싱을 검증한다.
    pub fn parse_response(json_str: &str) -> Option<String> {
        let v: Value = serde_json::from_str(json_str).ok()?;
        let text = v.get("response")?.as_str()?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    /// history 마지막 4개 항목으로 최근 대화 문자열을 만든다.
    fn format_recent(history: &[Event]) -> String {
        history
            .iter()
            .rev()
            .take(4)
            .rev()
            .map(|e| {
                let body = e
                    .content
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if body.is_empty() {
                    e.speaker.clone()
                } else {
                    format!("{}: {}", e.speaker, body)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl PersonaRuntime for OllamaBackend {
    /// 실제 Ollama LLM에 발화 텍스트 생성을 요청한다.
    ///
    /// - rng를 절대 소비하지 않는다 → 엔진 결정성(화자/침묵) 보존.
    /// - 네트워크 실패, 비2xx, JSON 파싱 실패, 타임아웃 → None 반환.
    /// - SECURITY: api_key는 Authorization 헤더에만 사용, 에러 메시지에 포함하지 않는다.
    fn generate(
        &mut self,
        speaker: &PersonaId,
        history: &[Event],
        _tick: u64,
        _rng: &mut ChaCha8Rng,
    ) -> Option<String> {
        let recent = Self::format_recent(history);
        let prompt = format!(
            "You are {speaker} in a casual group chat. Recent lines:\n{recent}\nReply with ONE short, in-character line. No preamble."
        );

        let system = self.system_prompts.get(speaker).map(String::as_str);

        let url = format!("{}/api/generate", self.endpoint);
        let body = Self::build_request_body(&self.model, &prompt, system);

        let mut req = self.client.post(&url).json(&body);

        // SECURITY: api_key는 Authorization 헤더에만 첨부한다. 에러 경로에 값을 절대 쓰지 않는다.
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }

        let response = match req.send() {
            Ok(r) => r,
            Err(_) => {
                eprintln!("[ollama] request failed (endpoint: {})", self.endpoint);
                return None;
            }
        };

        if !response.status().is_success() {
            eprintln!("[ollama] non-success status: {}", response.status());
            return None;
        }

        let text = match response.text() {
            Ok(t) => t,
            Err(_) => {
                eprintln!("[ollama] failed to read response body");
                return None;
            }
        };

        match Self::parse_response(&text) {
            Some(s) => Some(s),
            None => {
                eprintln!("[ollama] failed to parse response JSON");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn build_request_body_has_required_fields() {
        let body = OllamaBackend::build_request_body("gemma4:e4b", "Hello, who are you?", None);

        assert_eq!(body["model"], "gemma4:e4b");
        assert_eq!(body["prompt"], "Hello, who are you?");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn build_request_body_includes_system_when_some() {
        let body =
            OllamaBackend::build_request_body("m", "p", Some("you are X"));
        assert_eq!(body["model"], "m");
        assert_eq!(body["prompt"], "p");
        assert_eq!(body["stream"], false);
        assert_eq!(body["system"], "you are X");
    }

    #[test]
    fn build_request_body_omits_system_when_none() {
        let body = OllamaBackend::build_request_body("m", "p", None);
        assert!(
            body.get("system").is_none(),
            "system 필드가 None일 때 body에 포함되어서는 안 됨"
        );
    }

    #[test]
    fn system_prompts_map_returns_correct_prompt_per_speaker() {
        let mut map = BTreeMap::new();
        map.insert("friend".to_string(), "Be warm and friendly.".to_string());
        map.insert("chaos".to_string(), "Stir things up.".to_string());

        // 맵 직접 조회 검증
        assert_eq!(
            map.get("friend").map(String::as_str),
            Some("Be warm and friendly.")
        );
        assert_eq!(
            map.get("chaos").map(String::as_str),
            Some("Stir things up.")
        );
        // 없는 화자는 None
        assert_eq!(map.get("summarizer").map(String::as_str), None);
    }

    #[test]
    fn parse_response_extracts_text() {
        let json = r#"{"response":"hello there","done":true}"#;
        let result = OllamaBackend::parse_response(json);
        assert_eq!(result, Some("hello there".to_string()));
    }

    #[test]
    fn parse_response_trims_whitespace() {
        let json = r#"{"response":"  hi there  ","done":false}"#;
        let result = OllamaBackend::parse_response(json);
        assert_eq!(result, Some("hi there".to_string()));
    }

    #[test]
    fn parse_response_returns_none_on_invalid_json() {
        let result = OllamaBackend::parse_response("not json at all");
        assert_eq!(result, None);
    }

    #[test]
    fn parse_response_returns_none_on_missing_response_field() {
        let json = r#"{"done":true}"#;
        let result = OllamaBackend::parse_response(json);
        assert_eq!(result, None);
    }

    /// SECURITY 테스트: Debug 출력에 api_key 값이 절대 포함되지 않아야 한다.
    #[test]
    fn debug_output_does_not_leak_api_key() {
        let backend = OllamaBackend::new(
            "gemma4:e4b".to_string(),
            "http://localhost:11434".to_string(),
            Some("SECRET_TOKEN_123".to_string()),
            BTreeMap::new(),
            Duration::from_secs(30),
        );

        let debug_str = format!("{:?}", backend);

        assert!(
            !debug_str.contains("SECRET_TOKEN_123"),
            "SECURITY: api_key가 Debug 출력에 노출됨: {debug_str}"
        );
        // Some 여부는 표시되어야 한다
        assert!(
            debug_str.contains("redacted") || debug_str.contains("Some"),
            "Debug 출력이 api_key 존재 여부를 나타내야 함: {debug_str}"
        );
    }

    /// 실제 네트워크가 필요한 라이브 호출 테스트 — CI에서는 skip.
    #[test]
    #[ignore]
    fn live_generate_returns_some_string() {
        use rand::SeedableRng;
        let mut backend = OllamaBackend::new(
            "gemma4:e4b".to_string(),
            "http://localhost:11434".to_string(),
            None,
            BTreeMap::new(),
            Duration::from_secs(30),
        );
        let speaker = "friend".to_string();
        let history = Vec::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

        let result = backend.generate(&speaker, &history, 0, &mut rng);
        // 로컬 Ollama가 떠 있으면 Some, 없으면 None — 어느 쪽이든 panic 없어야 한다.
        println!("live result: {:?}", result);
    }
}
