//! OpenAI 호환 백엔드 (vLLM friend server, `/v1/chat/completions`).
//!
//! 지인서버는 vLLM이 OpenAI API를 에뮬레이션한다.
//! reasoning 모델(qwen3.6-35b)의 응답에는 `reasoning` 필드가 있지만 무시하고
//! `choices[0].message.content`만 추출한다.
//!
//! SECURITY: api_key는 Authorization 헤더에만 사용하며 Debug/로그/에러에 절대 노출하지 않는다.

use crate::model::{Event, PersonaId};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::Duration;

/// OpenAI `/v1/chat/completions` 호환 백엔드.
///
/// vLLM, OpenAI, 기타 호환 서버 모두 사용 가능.
/// SECURITY: `#[derive(Debug)]` 대신 수동 구현으로 api_key를 redacted 처리.
pub struct OpenAIBackend {
    client: reqwest::blocking::Client,
    model: String,
    endpoint: String,
    /// SECURITY: 로그/에러/Debug 출력에 절대 포함하지 않는다.
    api_key: Option<String>,
    system_prompts: BTreeMap<PersonaId, String>,
    /// OpenAI `max_tokens` 파라미터. None이면 body에서 생략(서버 기본값).
    max_tokens: Option<u64>,
}

/// SECURITY: api_key는 Some/None 여부만 표시한다. 값은 절대 출력하지 않는다.
impl std::fmt::Debug for OpenAIBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAIBackend")
            .field("model", &self.model)
            .field("endpoint", &self.endpoint)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

impl OpenAIBackend {
    /// 새 OpenAIBackend를 생성한다.
    ///
    /// - `model`: 모델 이름 (예: "qwen3.6-35b")
    /// - `endpoint`: 서버 주소 (예: "http://yongseek.iptime.org:8008"). `/v1/chat/completions` 경로는 자동 붙임.
    /// - `api_key`: Bearer 인증 키. 지인서버는 보통 None.
    /// - `system_prompts`: 화자별 system prompt 맵.
    /// - `timeout`: HTTP 요청 타임아웃.
    /// - `max_tokens`: OpenAI max_tokens 파라미터. None이면 생략.
    ///
    /// reqwest Client 빌드 실패 시 기본 Client로 폴백(panic 없음).
    pub fn new(
        model: String,
        endpoint: String,
        api_key: Option<String>,
        system_prompts: BTreeMap<PersonaId, String>,
        timeout: Duration,
        max_tokens: Option<u64>,
    ) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());

        Self { client, model, endpoint, api_key, system_prompts, max_tokens }
    }

    /// `/v1/chat/completions` 요청 body JSON을 조립한다.
    ///
    /// - `system`이 Some이면 messages 배열에 system 메시지를 먼저 추가한다.
    /// - `max_tokens`가 Some이면 body에 포함한다.
    /// - `stream: false`는 항상 포함한다.
    ///
    /// 네트워크 없이 직렬화 검증이 가능하도록 별도 함수로 분리.
    pub fn build_request_body(
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u64>,
    ) -> Value {
        let mut messages: Vec<Value> = Vec::new();

        // system 메시지: Some일 때만 추가
        if let Some(sys) = system {
            messages.push(serde_json::json!({
                "role": "system",
                "content": sys
            }));
        }

        // user 메시지는 항상 추가
        messages.push(serde_json::json!({
            "role": "user",
            "content": prompt
        }));

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": false,
            // 살롱은 짧은 스몰토크라 reasoning(CoT)을 끈다. qwen3 계열 vLLM은
            // chat_template_kwargs.enable_thinking=false를 받는다(쓰지 않는 템플릿은 무시).
            // reasoning을 켜두면 max_tokens를 CoT가 다 먹어 content가 빈 채 잘린다(검증 2026-06-02:
            // max_tokens 256 전부 reasoning 963자에 소모, content=null, finish_reason=length).
            "chat_template_kwargs": { "enable_thinking": false },
        });

        // max_tokens가 Some일 때만 추가
        if let Some(n) = max_tokens {
            body["max_tokens"] = serde_json::json!(n);
        }

        body
    }

    /// 응답 JSON에서 `choices[0].message.content`를 추출한다.
    ///
    /// reasoning 모델(qwen3.6-35b)의 응답에 `reasoning` 필드가 있어도 무시한다.
    /// 답은 항상 `content`에 있다.
    ///
    /// 네트워크 없이 파싱 검증이 가능하도록 별도 함수로 분리.
    pub fn parse_response(json_str: &str) -> Option<String> {
        let v: Value = serde_json::from_str(json_str).ok()?;
        let content = v
            .get("choices")?
            .get(0)?
            .get("message")?
            .get("content")?
            .as_str()?;
        let trimmed = content.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    /// history 마지막 4개 항목으로 최근 대화 문자열을 만든다(OllamaBackend와 동일 로직).
    fn format_recent(history: &[Event]) -> String {
        history
            .iter()
            .rev()
            .take(4)
            .rev()
            .map(|e| {
                let body = e.content.as_deref().unwrap_or("").trim().to_string();
                if body.is_empty() {
                    e.speaker.clone()
                } else {
                    format!("{}: {}", e.speaker, body)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// user 프롬프트를 조립한다(OllamaBackend::assemble_user_prompt와 동일 로직).
    ///
    /// 섹션 순서:
    ///   1. 공유 최근 로그 (`recent`)
    ///   2. 회상 슬롯 — `recall`이 Some(r)일 때만 `[기억]\n{r}` 삽입
    ///   3. 답변 지시문
    ///
    /// 라이브 경로에서만 recall=Some. driver/headless는 recall=None.
    fn assemble_user_prompt(recent: &str, recall: Option<&str>) -> String {
        let mut parts = Vec::with_capacity(3);
        parts.push(format!("Recent lines:\n{recent}"));
        if let Some(r) = recall {
            parts.push(format!("[기억]\n{r}"));
        }
        parts.push("Reply with ONE short, in-character line. No preamble.".to_string());
        parts.join("\n")
    }

    /// OpenAI 호환 서버에 발화 텍스트 생성을 요청한다.
    ///
    /// - `recall`: 라이브 경로에서만 Some으로 전달. driver/headless 경로는 항상 None.
    /// - rng를 소비하지 않는다 → 엔진 결정성 보존.
    /// - 에러/비2xx/타임아웃/파싱 실패 → None(panic 없음).
    /// - SECURITY: api_key는 Authorization 헤더에만, 에러 메시지에 절대 포함하지 않는다.
    pub fn generate(
        &self,
        speaker: &PersonaId,
        history: &[Event],
        _tick: u64,
        recall: Option<&str>,
    ) -> Option<String> {
        let recent = Self::format_recent(history);
        let user_prompt = Self::assemble_user_prompt(&recent, recall);

        let system = self.system_prompts.get(speaker).map(String::as_str);
        let url = format!("{}/v1/chat/completions", self.endpoint);
        let body = Self::build_request_body(&self.model, &user_prompt, system, self.max_tokens);

        let mut req = self.client.post(&url).json(&body);

        // SECURITY: api_key는 Authorization 헤더에만 첨부한다. 에러 경로에 값을 절대 쓰지 않는다.
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }

        let response = match req.send() {
            Ok(r) => r,
            Err(_) => {
                eprintln!("[openai] request failed (endpoint: {})", self.endpoint);
                return None;
            }
        };

        if !response.status().is_success() {
            eprintln!("[openai] non-success status: {}", response.status());
            return None;
        }

        let text = match response.text() {
            Ok(t) => t,
            Err(_) => {
                eprintln!("[openai] failed to read response body");
                return None;
            }
        };

        match Self::parse_response(&text) {
            Some(s) => Some(s),
            None => {
                eprintln!("[openai] failed to parse response JSON");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // -------------------------------------------------------------------------
    // build_request_body 테스트
    // -------------------------------------------------------------------------

    /// system=Some이면 messages 배열에 system 메시지가 먼저, user 메시지가 뒤에 온다.
    #[test]
    fn build_request_body_with_system_has_correct_messages() {
        let body = OpenAIBackend::build_request_body("qwen3.6-35b", "Hi", Some("You are X"), None);

        let messages = body["messages"].as_array().expect("messages는 배열이어야 함");
        assert_eq!(messages.len(), 2, "system+user 메시지 2개여야 함");
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are X");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Hi");
    }

    /// system=None이면 user 메시지만 있어야 한다.
    #[test]
    fn build_request_body_without_system_has_only_user_message() {
        let body = OpenAIBackend::build_request_body("m", "prompt", None, None);

        let messages = body["messages"].as_array().expect("messages는 배열이어야 함");
        assert_eq!(messages.len(), 1, "user 메시지만 있어야 함");
        assert_eq!(messages[0]["role"], "user");
    }

    /// max_tokens=Some이면 body에 max_tokens 필드가 포함된다.
    #[test]
    fn build_request_body_includes_max_tokens_when_some() {
        let body = OpenAIBackend::build_request_body("m", "p", None, Some(512));
        assert_eq!(body["max_tokens"], 512, "max_tokens가 512여야 함");
    }

    /// max_tokens=None이면 body에 max_tokens 필드가 없어야 한다.
    #[test]
    fn build_request_body_omits_max_tokens_when_none() {
        let body = OpenAIBackend::build_request_body("m", "p", None, None);
        assert!(
            body.get("max_tokens").is_none(),
            "max_tokens=None이면 body에 포함되어서는 안 됨"
        );
    }

    /// stream: false는 항상 포함된다.
    #[test]
    fn build_request_body_stream_is_false() {
        let body = OpenAIBackend::build_request_body("m", "p", None, None);
        assert_eq!(body["stream"], false);
    }

    /// reasoning(thinking)을 끄기 위해 chat_template_kwargs.enable_thinking=false가 항상 포함된다.
    /// (reasoning 모델이 max_tokens를 CoT로 소진해 content가 비는 것을 방지)
    #[test]
    fn build_request_body_disables_thinking() {
        let body = OpenAIBackend::build_request_body("m", "p", None, Some(256));
        assert_eq!(
            body["chat_template_kwargs"]["enable_thinking"], false,
            "enable_thinking=false가 body에 있어야 함"
        );
    }

    /// model 필드가 올바르게 설정된다.
    #[test]
    fn build_request_body_sets_model() {
        let body = OpenAIBackend::build_request_body("qwen3.6-35b", "p", None, None);
        assert_eq!(body["model"], "qwen3.6-35b");
    }

    // -------------------------------------------------------------------------
    // parse_response 테스트
    // -------------------------------------------------------------------------

    /// choices[0].message.content에서 텍스트를 추출한다.
    #[test]
    fn parse_response_extracts_content() {
        let json = r#"{
            "choices": [{"message": {"role": "assistant", "content": "Hello there"}}]
        }"#;
        let result = OpenAIBackend::parse_response(json);
        assert_eq!(result, Some("Hello there".to_string()));
    }

    /// reasoning 필드가 있어도 content만 추출한다(reasoning 모델 대응).
    #[test]
    fn parse_response_ignores_reasoning_field() {
        let json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning": "Let me think step by step... this is the chain of thought",
                    "content": "Just the answer"
                }
            }]
        }"#;
        let result = OpenAIBackend::parse_response(json);
        assert_eq!(result, Some("Just the answer".to_string()));
        // reasoning 텍스트가 결과에 포함되어서는 안 된다
        assert!(result.unwrap() != "Let me think step by step... this is the chain of thought");
    }

    /// content가 비면 None을 반환한다.
    #[test]
    fn parse_response_returns_none_on_empty_content() {
        let json = r#"{"choices": [{"message": {"role": "assistant", "content": "  "}}]}"#;
        let result = OpenAIBackend::parse_response(json);
        assert_eq!(result, None);
    }

    /// choices가 없으면 None을 반환한다.
    #[test]
    fn parse_response_returns_none_when_choices_missing() {
        let json = r#"{"id": "abc", "model": "x"}"#;
        let result = OpenAIBackend::parse_response(json);
        assert_eq!(result, None);
    }

    /// choices 배열이 비면 None을 반환한다.
    #[test]
    fn parse_response_returns_none_on_empty_choices() {
        let json = r#"{"choices": []}"#;
        let result = OpenAIBackend::parse_response(json);
        assert_eq!(result, None);
    }

    /// 잘못된 JSON이면 None을 반환한다.
    #[test]
    fn parse_response_returns_none_on_invalid_json() {
        let result = OpenAIBackend::parse_response("not json");
        assert_eq!(result, None);
    }

    /// 공백을 trim한다.
    #[test]
    fn parse_response_trims_whitespace() {
        let json = r#"{"choices": [{"message": {"role": "assistant", "content": "  hi there  "}}]}"#;
        let result = OpenAIBackend::parse_response(json);
        assert_eq!(result, Some("hi there".to_string()));
    }

    // -------------------------------------------------------------------------
    // SECURITY 테스트
    // -------------------------------------------------------------------------

    /// SECURITY: Debug 출력에 api_key 값이 포함되어서는 안 된다.
    #[test]
    fn debug_output_does_not_leak_api_key() {
        let backend = OpenAIBackend::new(
            "qwen3.6-35b".to_string(),
            "http://yongseek.iptime.org:8008".to_string(),
            Some("OPENAI_SECRET_KEY_999".to_string()),
            BTreeMap::new(),
            Duration::from_secs(30),
            None,
        );

        let debug_str = format!("{:?}", backend);

        assert!(
            !debug_str.contains("OPENAI_SECRET_KEY_999"),
            "SECURITY: api_key가 Debug 출력에 노출됨: {debug_str}"
        );
        assert!(
            debug_str.contains("redacted") || debug_str.contains("Some"),
            "Debug 출력이 api_key 존재 여부를 나타내야 함: {debug_str}"
        );
    }

    // -------------------------------------------------------------------------
    // task-41: recall 슬롯 테스트 (네트워크 없음)
    // -------------------------------------------------------------------------

    /// recall=Some이면 assemble_user_prompt의 user 메시지에 [기억] 섹션이 포함된다.
    /// 순서: 최근 로그 → [기억] → 지시문.
    #[test]
    fn assemble_user_prompt_with_recall_includes_memory_section() {
        let recent = "A: 안녕\nB: 반가워";
        let recall_text = "지난 대화에서:\n- A: 약속했어";

        let prompt = OpenAIBackend::assemble_user_prompt(recent, Some(recall_text));

        assert!(
            prompt.contains("[기억]"),
            "recall=Some이면 [기억] 섹션이 포함되어야 함"
        );
        assert!(
            prompt.contains(recall_text),
            "회상 텍스트가 포함되어야 함"
        );
        // 순서: 로그 < [기억] < 지시문
        let pos_log = prompt.find("Recent lines:").unwrap();
        let pos_mem = prompt.find("[기억]").unwrap();
        let pos_inst = prompt.find("Reply with ONE short").unwrap();
        assert!(pos_log < pos_mem, "로그가 [기억] 앞에 있어야 함");
        assert!(pos_mem < pos_inst, "[기억]이 지시문 앞에 있어야 함");
    }

    /// recall=None이면 assemble_user_prompt의 user 메시지에 [기억] 섹션이 없다.
    #[test]
    fn assemble_user_prompt_without_recall_has_no_memory_section() {
        let recent = "A: 안녕\nB: 반가워";

        let prompt = OpenAIBackend::assemble_user_prompt(recent, None);

        assert!(
            !prompt.contains("[기억]"),
            "recall=None이면 [기억] 섹션이 없어야 함"
        );
        assert!(
            prompt.contains("Recent lines:"),
            "최근 로그가 포함되어야 함"
        );
        assert!(
            prompt.contains("Reply with ONE short"),
            "지시문이 포함되어야 함"
        );
    }

    /// PersonaRuntime::generate(driver 경로)는 recall=None으로 호출한다.
    /// OpenAIBackend는 PersonaRuntime을 구현하지 않으므로(BackendPool이 함), BackendPool을 통해 확인.
    /// → pool::generate(PersonaRuntime)는 generate_one(..., None)을 호출함을 pool 테스트에서 검증.
    /// 여기서는 직접 generate(recall=None)가 [기억] 없는 프롬프트를 만드는지만 검증.
    #[test]
    fn generate_with_none_recall_produces_no_memory_section_in_prompt() {
        // assemble_user_prompt를 직접 호출해 recall=None이면 [기억] 섹션 없음을 확인한다.
        let recent = "A: hi";
        let prompt = OpenAIBackend::assemble_user_prompt(recent, None);
        assert!(
            !prompt.contains("[기억]"),
            "driver 경로(recall=None) → [기억] 섹션 없어야 함(골든 보존)"
        );
    }

    // -------------------------------------------------------------------------
    // 라이브 테스트 (네트워크 필요, CI에서는 skip)
    // -------------------------------------------------------------------------

    /// 지인서버에 실제 generate를 보내는 라이브 테스트.
    /// SALON_FRIEND_ENDPOINT (기본: http://yongseek.iptime.org:8008)
    /// SALON_FRIEND_MODEL    (기본: qwen3.6-35b)
    ///
    /// `cargo test -- friend_server_live_generate --ignored` 로 수동 실행.
    #[test]
    #[ignore]
    fn friend_server_live_generate() {
        let endpoint = std::env::var("SALON_FRIEND_ENDPOINT")
            .unwrap_or_else(|_| "http://yongseek.iptime.org:8008".to_string());
        // 기본은 reasoning이 꺼진 -fast 변형(친구 안내). 일반 qwen3.6-35b도
        // build_request_body의 enable_thinking=false로 안전하게 동작한다.
        let model = std::env::var("SALON_FRIEND_MODEL")
            .unwrap_or_else(|_| "qwen3.6-35b-fast".to_string());

        let backend = OpenAIBackend::new(
            model.clone(),
            endpoint.clone(),
            None, // 지인서버 무인증
            BTreeMap::new(),
            Duration::from_secs(120),
            Some(256),
        );

        let speaker = "friend".to_string();
        let history = vec![
            crate::model::Event {
                ts: 0.0,
                speaker: "chaos".to_string(),
                mark: 1.0,
                content: Some("The salon is getting lively tonight.".to_string()),
            },
        ];

        let result = backend.generate(&speaker, &history, 0, None);
        println!("live friend-server result (model={model}, endpoint={endpoint}): {result:?}");
        // 서버가 응답하면 Some(텍스트), 오프라인이면 None — 어느 쪽이든 panic 없어야 한다.
        // Architect가 수동으로 Some 여부를 확인한다.
    }
}
