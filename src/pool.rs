//! 백엔드 풀 (v0.4 task-21/22).
//!
//! task-21: 데이터 구조 + 생성자.
//! task-22: `persona -> backend_name` 라우팅 + `impl PersonaRuntime`. 세마포어/배치는 task-23.

use crate::model::{Event, PersonaId};
use crate::ollama::OllamaBackend;
use crate::runtime::PersonaRuntime;
use rand_chacha::ChaCha8Rng;
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

/// 이름붙은 백엔드 레지스트리 (v0.4 task-21/22).
///
/// - task-21: 데이터 구조 + 생성자.
/// - task-22: `persona -> backend_name` 라우팅 + `impl PersonaRuntime`.
/// - task-23: 백엔드별 세마포어 + 배치 API.
pub struct BackendPool {
    /// name -> OllamaBackend 맵
    backends: BTreeMap<String, OllamaBackend>,
    /// 기본 백엔드 이름. 라우팅 미지정 페르소나에 사용한다.
    default_backend: Option<String>,
    /// persona_id -> backend_name 라우팅 맵. 비어 있으면 모든 페르소나가 default로 향한다.
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

    /// BackendConfig + system_prompts로 OllamaBackend를 빌드해 풀에 등록한다.
    ///
    /// - `system_prompts`: 화자별 system prompt 맵. 백엔드는 라우팅된 페르소나에게만
    ///   사용하지만 전체 맵을 넘겨도 무해하다(OllamaBackend가 speaker로 조회).
    /// - 같은 이름으로 두 번 호출하면 덮어쓴다.
    /// - OllamaBackend 빌드 실패는 폴백(panic 없음) — reqwest::Client::new()로 대체된다.
    pub fn add(&mut self, config: BackendConfig, system_prompts: BTreeMap<PersonaId, String>) {
        let backend = OllamaBackend::new(
            config.model,
            config.endpoint,
            config.api_key,
            system_prompts,
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

    /// persona_id -> backend_name 라우팅 엔트리를 추가한다.
    ///
    /// 같은 persona_id로 두 번 호출하면 덮어쓴다.
    pub fn add_route(&mut self, persona_id: impl Into<String>, backend_name: impl Into<String>) {
        self.routing.insert(persona_id.into(), backend_name.into());
    }

    /// 주어진 speaker에 대해 실제로 사용할 백엔드 이름을 해석한다(네트워크 없음).
    ///
    /// 해석 순서:
    ///   1. routing 맵에 speaker 항목이 있으면 그 이름 반환.
    ///   2. 없으면 default_backend 이름 반환.
    ///   3. default_backend도 None이면 None 반환(panic 없음).
    ///
    /// 반환값이 Some이더라도 실제로 backends 맵에 해당 이름이 존재해야 generate가 성공한다.
    /// 이름 해석과 존재 확인을 분리함으로써 단위 테스트에서 네트워크 없이 라우팅 로직만 검증한다.
    pub fn resolve(&self, speaker: &str) -> Option<&str> {
        self.routing
            .get(speaker)
            .map(String::as_str)
            .or_else(|| self.default_backend.as_deref())
    }
}

impl PersonaRuntime for BackendPool {
    /// speaker를 routing → default 순서로 백엔드에 라우팅해 generate를 위임한다.
    ///
    /// - rng를 소비하지 않는다(내부 OllamaBackend도 미소비) → 엔진 결정성 보존.
    /// - 라우팅 대상 백엔드가 backends 맵에 없거나 default도 없으면 None 반환(panic 없음).
    fn generate(
        &mut self,
        speaker: &PersonaId,
        history: &[Event],
        tick: u64,
        rng: &mut ChaCha8Rng,
    ) -> Option<String> {
        // resolve는 &self를 빌리므로 이름을 String으로 복사해 가변 borrow 충돌을 피한다.
        let backend_name = self.resolve(speaker)?.to_string();
        let backend = self.backends.get_mut(&backend_name)?;
        backend.generate(speaker, history, tick, rng)
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
    use crate::runtime::PersonaRuntime;
    use std::time::Duration;

    /// 테스트용 헬퍼: 지정 이름으로 BackendConfig를 만든다(빠른 타임아웃, 오프라인).
    fn make_config(name: &str) -> BackendConfig {
        BackendConfig::new(
            name,
            "fake-model",
            "http://127.0.0.1:1", // 연결 불가 주소 — 네트워크 호출 없이 라우팅만 테스트
            None,
            1,
            None,
            Duration::from_millis(1),
        )
    }

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
        pool.add(cfg, BTreeMap::new());

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
        pool.add(cfg, BTreeMap::new());
        pool.set_default("cloud");

        assert_eq!(pool.default_backend_name(), Some("cloud"));
    }

    #[test]
    fn backend_pool_multiple_backends() {
        let mut pool = BackendPool::new();

        pool.add(
            BackendConfig::new(
                "cloud",
                "gemma4:e4b",
                "https://api.ollama.ai",
                None,
                3,
                None,
                Duration::from_secs(30),
            ),
            BTreeMap::new(),
        );
        pool.add(
            BackendConfig::new(
                "qwen",
                "qwen3.6:32b",
                "http://friend-server:11434",
                None,
                2,
                Some(100_000),
                Duration::from_secs(60),
            ),
            BTreeMap::new(),
        );

        assert_eq!(pool.len(), 2);
        let names = pool.backend_names();
        assert!(names.contains(&"cloud"));
        assert!(names.contains(&"qwen"));
    }

    // -------------------------------------------------------------------------
    // task-22: resolve() 라우팅 단위 테스트 (네트워크 불필요)
    // -------------------------------------------------------------------------

    /// routing에 등록된 persona는 지정 백엔드 이름을 반환한다.
    #[test]
    fn resolve_returns_routed_backend_for_mapped_persona() {
        let mut pool = BackendPool::new();
        pool.add(make_config("cloud"), BTreeMap::new());
        pool.add(make_config("qwen"), BTreeMap::new());
        pool.set_default("cloud");
        pool.add_route("summarizer", "qwen");

        assert_eq!(pool.resolve("summarizer"), Some("qwen"));
    }

    /// routing에 없는 persona는 default 백엔드 이름을 반환한다.
    #[test]
    fn resolve_falls_back_to_default_for_unmapped_persona() {
        let mut pool = BackendPool::new();
        pool.add(make_config("cloud"), BTreeMap::new());
        pool.set_default("cloud");
        // "friend"는 routing에 없음

        assert_eq!(pool.resolve("friend"), Some("cloud"));
        assert_eq!(pool.resolve("chaos"), Some("cloud"));
    }

    /// routing도 없고 default도 None이면 None을 반환한다(panic 없음).
    #[test]
    fn resolve_returns_none_when_no_default_and_no_route() {
        let pool = BackendPool::new(); // default 없음, routing 없음

        assert_eq!(pool.resolve("friend"), None);
    }

    /// BackendPool이 &mut dyn PersonaRuntime으로 사용 가능한지 확인한다.
    /// 실제 네트워크 없이 타입 호환성만 검증하므로 generate는 None을 반환하는 게 정상.
    #[test]
    fn backend_pool_is_usable_as_dyn_persona_runtime() {
        use rand::SeedableRng;

        let mut pool = BackendPool::new();
        // 오프라인 주소(1번 포트)이므로 generate는 None을 반환한다.
        pool.add(make_config("cloud"), BTreeMap::new());
        pool.set_default("cloud");

        // dyn PersonaRuntime으로 캐스팅 가능해야 한다.
        let runtime: &mut dyn PersonaRuntime = &mut pool;

        let speaker = "friend".to_string();
        let history = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        // 네트워크 없음 → None 반환. panic이 없으면 테스트 통과.
        let result = runtime.generate(&speaker, &history, 0, &mut rng);
        assert_eq!(result, None, "오프라인 백엔드는 None을 반환해야 함");
    }

    /// generate가 routing/default 모두 없으면 None을 반환한다(panic 없음).
    #[test]
    fn generate_returns_none_when_no_backend_resolved() {
        use rand::SeedableRng;

        let mut pool = BackendPool::new(); // 백엔드 없음, default 없음
        let speaker = "friend".to_string();
        let history = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        let result = pool.generate(&speaker, &history, 0, &mut rng);
        assert_eq!(result, None);
    }
}
