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
    /// 백엔드별 컨텍스트 윈도우 크기.
    /// None이면 요청 body에서 options.num_ctx를 완전히 생략(cloud/원격 auto-max).
    /// Some(n)이면 options.num_ctx = n (로컬 e4b의 경우 RAM 상한 8192).
    num_ctx: Option<u64>,
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
    /// - `num_ctx`: 컨텍스트 윈도우 크기. None이면 요청 body에서 생략(cloud auto-max). Some(n)이면 options.num_ctx = n.
    ///
    /// reqwest Client 빌드에 실패하면 기본 Client로 폴백한다(panic 없음).
    pub fn new(
        model: String,
        endpoint: String,
        api_key: Option<String>,
        system_prompts: BTreeMap<PersonaId, String>,
        timeout: Duration,
        num_ctx: Option<u64>,
    ) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self { client, model, endpoint, api_key, system_prompts, num_ctx }
    }

    /// user 프롬프트를 섹션 순서대로 조립한다.
    ///
    /// 섹션 순서:
    ///   1. 공유 최근 로그 (`recent`)
    ///   2. 회상 슬롯 — `recall`이 Some(r)일 때만 `[기억]\n{r}` 삽입 (장기기억 엔진 예약 슬롯)
    ///   3. 답변 지시문
    ///
    /// 페르소나 지시(system prompt)는 이 함수가 아닌 `system` 필드로 전달한다.
    fn assemble_user_prompt(recent: &str, recall: Option<&str>) -> String {
        let mut parts = Vec::with_capacity(3);
        parts.push(format!("Recent lines:\n{recent}"));
        if let Some(r) = recall {
            parts.push(format!("[기억]\n{r}"));
        }
        parts.push("Reply with ONE short, in-character line. No preamble.".to_string());
        parts.join("\n")
    }

    /// `/api/generate` 요청 body JSON을 조립한다.
    ///
    /// - `system`이 Some이면 body에 `"system"` 필드를 추가한다.
    /// - None이면 `"system"` 필드를 완전히 생략한다.
    /// - `num_ctx`가 Some(n)이면 `options.num_ctx = n`을 설정한다.
    ///   None이면 options.num_ctx를 생략한다(cloud/원격이 모델 최대 ctx로 auto-max).
    ///   options에 설정할 항목이 없으면 options 키 자체를 생략한다.
    ///
    /// 별도 함수로 분리해 테스트에서 네트워크 없이 직렬화를 검증한다.
    pub fn build_request_body(
        model: &str,
        prompt: &str,
        system: Option<&str>,
        num_ctx: Option<u64>,
    ) -> Value {
        let mut body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
        });
        if let Some(n) = num_ctx {
            body["options"] = serde_json::json!({ "num_ctx": n });
        }
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

impl OllamaBackend {
    /// 발화 텍스트 생성 본문(rng 불요 · &self).
    ///
    /// `PersonaRuntime::generate`에서 위임 호출한다. Backend enum 디스패치에서도 직접 호출.
    /// - `recall`: 라이브 경로에서만 Some으로 전달. driver/headless 경로는 항상 None.
    /// - rng를 소비하지 않으므로 엔진 결정성이 보존된다.
    /// - 네트워크 실패, 비2xx, JSON 파싱 실패, 타임아웃 → None 반환(panic 없음).
    /// - SECURITY: api_key는 Authorization 헤더에만, 에러 메시지에 절대 포함하지 않는다.
    pub fn generate_shared(
        &self,
        speaker: &PersonaId,
        history: &[Event],
        _tick: u64,
        recall: Option<&str>,
    ) -> Option<String> {
        let recent = Self::format_recent(history);
        let user_prompt = Self::assemble_user_prompt(&recent, recall);

        let system = self.system_prompts.get(speaker).map(String::as_str);

        let url = format!("{}/api/generate", self.endpoint);
        let body = Self::build_request_body(&self.model, &user_prompt, system, self.num_ctx);

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

impl PersonaRuntime for OllamaBackend {
    /// 실제 Ollama LLM에 발화 텍스트 생성을 요청한다. generate_shared에 위임.
    ///
    /// - driver/headless 경로: recall=None(회상 미주입 → 골든 보존).
    /// - rng를 절대 소비하지 않는다 → 엔진 결정성(화자/침묵) 보존.
    fn generate(
        &mut self,
        speaker: &PersonaId,
        history: &[Event],
        tick: u64,
        _rng: &mut ChaCha8Rng,
    ) -> Option<String> {
        self.generate_shared(speaker, history, tick, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn build_request_body_has_required_fields() {
        let body =
            OllamaBackend::build_request_body("gemma4:e4b", "Hello, who are you?", None, None);

        assert_eq!(body["model"], "gemma4:e4b");
        assert_eq!(body["prompt"], "Hello, who are you?");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn build_request_body_includes_system_when_some() {
        let body = OllamaBackend::build_request_body("m", "p", Some("you are X"), None);
        assert_eq!(body["model"], "m");
        assert_eq!(body["prompt"], "p");
        assert_eq!(body["stream"], false);
        assert_eq!(body["system"], "you are X");
    }

    #[test]
    fn build_request_body_omits_system_when_none() {
        let body = OllamaBackend::build_request_body("m", "p", None, None);
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
            Some(8192),
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

    /// assemble_user_prompt: recall=None이면 회상 섹션 없이 로그+지시만 포함.
    /// recall=Some이면 회상 텍스트가 로그 뒤, 지시 앞에 삽입된다.
    #[test]
    fn assemble_user_prompt_sections_ordering() {
        let log = "Recent lines:\nA: hi";

        // recall=None: 회상 섹션 없음
        let without_recall = OllamaBackend::assemble_user_prompt(log, None);
        assert!(
            without_recall.contains("Recent lines:\nA: hi"),
            "로그가 포함되어야 함"
        );
        assert!(
            without_recall.contains("Reply with ONE short"),
            "지시문이 포함되어야 함"
        );
        assert!(
            !without_recall.contains("[기억]"),
            "recall=None일 때 [기억] 섹션이 없어야 함"
        );

        // recall=Some: 회상 텍스트가 로그 뒤, 지시 앞에 위치
        let recall_text = "과거: 약속함";
        let with_recall = OllamaBackend::assemble_user_prompt(log, Some(recall_text));
        assert!(
            with_recall.contains(recall_text),
            "회상 텍스트가 포함되어야 함"
        );
        let pos_log = with_recall.find("Recent lines:").unwrap();
        let pos_recall = with_recall.find(recall_text).unwrap();
        let pos_instruction = with_recall.find("Reply with ONE short").unwrap();
        assert!(
            pos_log < pos_recall,
            "로그(pos={pos_log})가 회상(pos={pos_recall}) 앞에 있어야 함"
        );
        assert!(
            pos_recall < pos_instruction,
            "회상(pos={pos_recall})이 지시문(pos={pos_instruction}) 앞에 있어야 함"
        );
    }

    /// build_request_body의 num_ctx가 Some(8192)이면 options.num_ctx가 8192로 설정된다.
    #[test]
    fn build_request_body_sets_num_ctx() {
        let body = OllamaBackend::build_request_body("m", "p", None, Some(8192));
        let num_ctx = body
            .get("options")
            .and_then(|o| o.get("num_ctx"))
            .and_then(|v| v.as_u64());
        assert_eq!(
            num_ctx,
            Some(8192),
            "options.num_ctx가 8192여야 함, 실제: {:?}",
            num_ctx
        );
    }

    /// build_request_body의 num_ctx가 None이면 options.num_ctx가 body에 없어야 한다.
    #[test]
    fn build_request_body_omits_num_ctx_when_none() {
        let body = OllamaBackend::build_request_body("m", "p", None, None);
        let has_num_ctx = body
            .get("options")
            .and_then(|o| o.get("num_ctx"))
            .is_some();
        assert!(
            !has_num_ctx,
            "num_ctx=None이면 options.num_ctx가 body에 포함되어서는 안 됨"
        );
        // options 키 자체도 없어야 한다
        assert!(
            body.get("options").is_none(),
            "num_ctx=None이고 다른 options 없으면 options 키 자체가 없어야 함"
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
            Some(8192),
        );
        let speaker = "friend".to_string();
        let history = Vec::new();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(42);

        let result = backend.generate(&speaker, &history, 0, &mut rng);
        // 로컬 Ollama가 떠 있으면 Some, 없으면 None — 어느 쪽이든 panic 없어야 한다.
        println!("live result: {:?}", result);
    }
}
