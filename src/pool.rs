//! 백엔드 풀 스켈레톤 (v0.4 task-21).
//!
//! 이 task에서는 데이터 구조와 생성자만 정의한다.
//! 라우팅(`generate_one`)은 task-22, 세마포어/배치는 task-23에서 추가한다.

use crate::ollama::OllamaBackend;
use std::collections::BTreeMap;
use std::time::Duration;

/// 개별 백엔드의 구성 파라미터.
///
/// 백엔드별로 모델·엔드포인트·인증·동시성 상한·ctx를 독립적으로 관리한다.
///
/// SECURITY: `#[derive(Debug)]`를 쓰면 api_key가 노출되므로 수동 구현.
pub struct BackendConfig {
    /// 풀 안에서 백엔드를 식별하는 이름 (예: "cloud", "qwen", "local")
    pub name: String,
    /// Ollama 모델 이름 (예: "gemma4:e4b", "qwen3.6:32b")
    pub model: String,
    /// Ollama 서버 엔드포인트 (예: "http://localhost:11434", "https://api.ollama.ai")
    pub endpoint: String,
    /// Ollama Cloud 인증 키. None이면 Authorization 헤더 없음.
    /// SECURITY: 로그/에러/Debug 출력에 절대 노출하지 않는다.
    pub api_key: Option<String>,
    /// 동시 in-flight 상한. task-23에서 세마포어로 집행한다.
    /// cloud=3, qwen=2, 로컬=1(순차)
    pub max_concurrent: usize,
    /// 컨텍스트 윈도우 크기.
    /// None이면 요청 body에서 생략(cloud/원격 auto-max).
    /// Some(n)이면 options.num_ctx = n (로컬 e4b의 경우 RAM 상한 8192).
    pub num_ctx: Option<u64>,
    /// HTTP 요청 타임아웃
    pub timeout: Duration,
}

/// SECURITY: api_key를 절대 출력하지 않는다. Some/None 여부만 표시한다.
impl std::fmt::Debug for BackendConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendConfig")
            .field("name", &self.name)
            .field("model", &self.model)
            .field("endpoint", &self.endpoint)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("max_concurrent", &self.max_concurrent)
            .field("num_ctx", &self.num_ctx)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl BackendConfig {
    /// 새 BackendConfig를 생성한다.
    pub fn new(
        name: impl Into<String>,
        model: impl Into<String>,
        endpoint: impl Into<String>,
        api_key: Option<String>,
        max_concurrent: usize,
        num_ctx: Option<u64>,
        timeout: Duration,
    ) -> Self {
        Self {
            name: name.into(),
            model: model.into(),
            endpoint: endpoint.into(),
            api_key,
            max_concurrent,
            num_ctx,
            timeout,
        }
    }
}

/// 이름붙은 백엔드 레지스트리 스켈레톤 (v0.4 task-21).
///
/// - task-21(이 task): 데이터 구조 + 생성자만. 컴파일·단위 테스트 통과.
/// - task-22: `persona -> backend_name` 라우팅 + `generate_one`.
/// - task-23: 백엔드별 세마포어 + 배치 API.
///
/// 현재 `backends` 맵과 `default_backend` 이름만 보유한다.
/// `routing` 맵은 자리 예약(task-22에서 채움).
pub struct BackendPool {
    /// name -> OllamaBackend 맵
    backends: BTreeMap<String, OllamaBackend>,
    /// 기본 백엔드 이름. 라우팅 미지정 페르소나에 사용한다(task-22).
    default_backend: Option<String>,
    /// persona_id -> backend_name 라우팅 맵 (자리 예약, task-22에서 채움)
    #[allow(dead_code)]
    routing: BTreeMap<String, String>,
}

impl BackendPool {
    /// 빈 BackendPool을 생성한다.
    pub fn new() -> Self {
        Self {
            backends: BTreeMap::new(),
            default_backend: None,
            routing: BTreeMap::new(),
        }
    }

    /// BackendConfig로 OllamaBackend를 빌드해 풀에 등록한다.
    ///
    /// 같은 이름으로 두 번 호출하면 덮어쓴다.
    /// OllamaBackend 빌드 실패는 폴백(panic 없음) — reqwest::Client::new()로 대체된다.
    pub fn add(&mut self, config: BackendConfig) {
        use std::collections::BTreeMap as Bm;
        let backend = OllamaBackend::new(
            config.model,
            config.endpoint,
            config.api_key,
            Bm::new(), // system_prompts: task-22에서 라우팅과 함께 설정
            config.timeout,
            config.num_ctx,
        );
        self.backends.insert(config.name, backend);
    }

    /// 기본 백엔드 이름을 설정한다.
    ///
    /// 풀에 등록된 이름이어야 하지만 지금은 검증하지 않는다(task-22에서 라우팅 확정 시 검증).
    pub fn set_default(&mut self, name: impl Into<String>) {
        self.default_backend = Some(name.into());
    }

    /// 등록된 백엔드 이름 목록을 반환한다(정렬된 순서).
    pub fn backend_names(&self) -> Vec<&str> {
        self.backends.keys().map(String::as_str).collect()
    }

    /// 기본 백엔드 이름을 반환한다.
    pub fn default_backend_name(&self) -> Option<&str> {
        self.default_backend.as_deref()
    }

    /// 등록된 백엔드 수를 반환한다.
    pub fn len(&self) -> usize {
        self.backends.len()
    }

    /// 풀이 비어 있으면 true.
    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }
}

impl Default for BackendPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn backend_config_holds_fields() {
        let cfg = BackendConfig::new(
            "cloud",
            "gemma4:e4b",
            "https://api.ollama.ai",
            Some("SECRET".to_string()),
            3,
            None,
            Duration::from_secs(30),
        );

        assert_eq!(cfg.name, "cloud");
        assert_eq!(cfg.model, "gemma4:e4b");
        assert_eq!(cfg.endpoint, "https://api.ollama.ai");
        assert!(cfg.api_key.is_some());
        assert_eq!(cfg.max_concurrent, 3);
        assert!(cfg.num_ctx.is_none());
        assert_eq!(cfg.timeout, Duration::from_secs(30));
    }

    #[test]
    fn backend_config_local_holds_num_ctx() {
        let cfg = BackendConfig::new(
            "local",
            "gemma4:e4b",
            "http://localhost:11434",
            None,
            1,
            Some(8192),
            Duration::from_secs(30),
        );

        assert_eq!(cfg.num_ctx, Some(8192));
        assert!(cfg.api_key.is_none());
    }

    /// SECURITY 테스트: BackendConfig의 Debug 출력에 api_key 값이 포함되지 않아야 한다.
    #[test]
    fn backend_config_debug_does_not_leak_api_key() {
        let cfg = BackendConfig::new(
            "cloud",
            "gemma4:e4b",
            "https://api.ollama.ai",
            Some("SUPER_SECRET_KEY".to_string()),
            3,
            None,
            Duration::from_secs(30),
        );

        let debug_str = format!("{:?}", cfg);

        assert!(
            !debug_str.contains("SUPER_SECRET_KEY"),
            "SECURITY: api_key가 Debug 출력에 노출됨: {debug_str}"
        );
        assert!(
            debug_str.contains("redacted") || debug_str.contains("Some"),
            "Debug 출력이 api_key 존재 여부를 나타내야 함: {debug_str}"
        );
    }

    #[test]
    fn backend_pool_starts_empty() {
        let pool = BackendPool::new();
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
        assert!(pool.default_backend_name().is_none());
    }

    #[test]
    fn backend_pool_add_and_len() {
        let mut pool = BackendPool::new();

        let cfg = BackendConfig::new(
            "local",
            "gemma4:e4b",
            "http://localhost:11434",
            None,
            1,
            Some(8192),
            Duration::from_secs(30),
        );
        pool.add(cfg);

        assert_eq!(pool.len(), 1);
        assert!(!pool.is_empty());
    }

    #[test]
    fn backend_pool_set_default() {
        let mut pool = BackendPool::new();

        let cfg = BackendConfig::new(
            "cloud",
            "gemma4:e4b",
            "https://api.ollama.ai",
            None,
            3,
            None,
            Duration::from_secs(30),
        );
        pool.add(cfg);
        pool.set_default("cloud");

        assert_eq!(pool.default_backend_name(), Some("cloud"));
    }

    #[test]
    fn backend_pool_multiple_backends() {
        let mut pool = BackendPool::new();

        pool.add(BackendConfig::new(
            "cloud",
            "gemma4:e4b",
            "https://api.ollama.ai",
            None,
            3,
            None,
            Duration::from_secs(30),
        ));
        pool.add(BackendConfig::new(
            "qwen",
            "qwen3.6:32b",
            "http://friend-server:11434",
            None,
            2,
            Some(100_000),
            Duration::from_secs(60),
        ));

        assert_eq!(pool.len(), 2);
        let names = pool.backend_names();
        assert!(names.contains(&"cloud"));
        assert!(names.contains(&"qwen"));
    }
}
