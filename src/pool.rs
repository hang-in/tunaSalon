//! 백엔드 풀 (v0.4 task-21/22/27/23/24).
//!
//! task-21: 데이터 구조 + 생성자.
//! task-22: `persona -> backend_name` 라우팅 + `impl PersonaRuntime`. 세마포어/배치는 task-23.
//! task-27: `Protocol` + `Backend` enum(Ollama|OpenAI) + `OpenAIBackend` 통합.
//! task-23: 백엔드별 `Arc<Semaphore>` + `generate_batch` 병렬 배치 API.
//! task-24: 폴백 체인 (`fallbacks` 맵 + `fallback_chain` + generate/generate_batch 통합).

use crate::model::{Event, PersonaId};
use crate::ollama::OllamaBackend;
use crate::openai::OpenAIBackend;
use crate::runtime::PersonaRuntime;
use crate::semaphore::Semaphore;
use rand_chacha::ChaCha8Rng;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// 백엔드 프로토콜 종류.
///
/// - `Ollama`: Ollama `/api/generate` 프로토콜.
/// - `OpenAI`: OpenAI 호환 `/v1/chat/completions` 프로토콜(vLLM 등).
#[derive(Debug, Clone, PartialEq)]
pub enum Protocol {
    Ollama,
    OpenAI,
}

/// 이종 백엔드 추상 enum.
///
/// - `Backend::generate(&self, ...)`: 프로토콜에 따라 OllamaBackend::generate_shared 또는
///   OpenAIBackend::generate로 디스패치한다.
/// - rng를 소비하지 않는다 → 엔진 결정성 보존.
/// - Send + Sync: task-23 배치에서 스레드 간 공유 대비.
pub enum Backend {
    Ollama(OllamaBackend),
    OpenAI(OpenAIBackend),
}

impl Backend {
    /// 프로토콜에 맞게 발화 텍스트 생성을 위임한다. rng 불요.
    ///
    /// - `recall`: 라이브 경로(generate_one 경유)에서만 Some. generate_batch/PersonaRuntime은 None.
    /// - `system_prompt_override`: Some(p)이면 p를 system prompt로 사용, None이면 백엔드 내부 맵 조회(기존 동작).
    pub fn generate(
        &self,
        speaker: &PersonaId,
        history: &[Event],
        tick: u64,
        recall: Option<&str>,
        system_prompt_override: Option<&str>,
    ) -> Option<String> {
        match self {
            Backend::Ollama(b) => {
                b.generate_shared(speaker, history, tick, recall, system_prompt_override)
            }
            Backend::OpenAI(b) => {
                b.generate(speaker, history, tick, recall, system_prompt_override)
            }
        }
    }
}

/// 개별 백엔드의 구성 파라미터.
///
/// 백엔드별로 모델·엔드포인트·인증·동시성 상한·ctx를 독립적으로 관리한다.
///
/// SECURITY: `#[derive(Debug)]`를 쓰면 api_key가 노출되므로 수동 구현.
pub struct BackendConfig {
    /// 풀 안에서 백엔드를 식별하는 이름 (예: "cloud", "qwen", "local")
    pub name: String,
    /// 모델 이름 (예: "gemma4:e4b", "qwen3.6-35b")
    pub model: String,
    /// 서버 엔드포인트 (예: "http://localhost:11434", "http://yongseek.iptime.org:8008")
    pub endpoint: String,
    /// 인증 키. None이면 Authorization 헤더 없음.
    /// SECURITY: 로그/에러/Debug 출력에 절대 노출하지 않는다.
    pub api_key: Option<String>,
    /// 동시 in-flight 상한. task-23에서 세마포어로 집행한다.
    /// cloud=3, qwen=1(max-num-seqs 1), 로컬=1(순차)
    pub max_concurrent: usize,
    /// Ollama 컨텍스트 윈도우 크기. Ollama 프로토콜 전용.
    /// None이면 요청 body에서 생략(cloud/원격 auto-max).
    /// Some(n)이면 options.num_ctx = n (로컬 e4b의 경우 RAM 상한 8192).
    pub num_ctx: Option<u64>,
    /// HTTP 요청 타임아웃
    pub timeout: Duration,
    /// 백엔드 프로토콜 종류. new()의 기본값은 Ollama.
    pub protocol: Protocol,
    /// OpenAI max_tokens 파라미터. OpenAI 프로토콜 전용. None이면 생략.
    pub max_tokens: Option<u64>,
    /// thinking(reasoning) 모드. true이면 생성 요청에 thinking을 활성화한다.
    /// Ollama: body에 "think": true 추가. OpenAI: enable_thinking = true.
    /// 기본값 false(기존 동작 보존). main에서 config 생성 후 필드 직접 set.
    pub thinking: bool,
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
            .field("protocol", &self.protocol)
            .field("max_tokens", &self.max_tokens)
            .field("thinking", &self.thinking)
            .finish()
    }
}

impl BackendConfig {
    /// 새 BackendConfig를 생성한다(Ollama 프로토콜 기본).
    ///
    /// 기존 호출처(task-21/22 테스트·main.rs)가 그대로 컴파일된다.
    /// protocol=Ollama, max_tokens=None으로 자동 채워진다.
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
            protocol: Protocol::Ollama,
            max_tokens: None,
            thinking: false,
        }
    }

    /// OpenAI 호환 백엔드용 BackendConfig를 생성한다.
    ///
    /// protocol=OpenAI, num_ctx=None(OpenAI 프로토콜에 해당 없음)으로 설정.
    pub fn new_openai(
        name: impl Into<String>,
        model: impl Into<String>,
        endpoint: impl Into<String>,
        api_key: Option<String>,
        max_concurrent: usize,
        max_tokens: Option<u64>,
        timeout: Duration,
    ) -> Self {
        Self {
            name: name.into(),
            model: model.into(),
            endpoint: endpoint.into(),
            api_key,
            max_concurrent,
            num_ctx: None, // OpenAI 프로토콜에 해당 없음
            timeout,
            protocol: Protocol::OpenAI,
            max_tokens,
            thinking: false,
        }
    }
}

/// 이름붙은 백엔드 레지스트리 (v0.4 task-21/22/27/23/24).
///
/// - task-21: 데이터 구조 + 생성자.
/// - task-22: `persona -> backend_name` 라우팅 + `impl PersonaRuntime`.
/// - task-27: `Backend` enum(Ollama|OpenAI)으로 이종 프로토콜 지원.
/// - task-23: 백엔드별 `Arc<Semaphore>` + `generate_batch` 병렬 배치 API.
/// - task-24: 폴백 체인 — 실패(None) 시 다음 백엔드로 전환, 사이클 안전.
pub struct BackendPool {
    /// name -> Backend 맵 (Ollama|OpenAI 이종 가능)
    backends: BTreeMap<String, Backend>,
    /// name -> 백엔드별 동시성 세마포어. `add`에서 `config.max_concurrent`로 생성된다.
    /// `generate_batch`가 in-flight 상한을 집행하는 데 사용한다.
    semaphores: BTreeMap<String, Arc<Semaphore>>,
    /// 기본 백엔드 이름. 라우팅 미지정 페르소나에 사용한다.
    default_backend: Option<String>,
    /// persona_id -> backend_name 라우팅 맵. 비어 있으면 모든 페르소나가 default로 향한다.
    routing: BTreeMap<String, String>,
    /// backend_name -> 폴백 backend_name 맵 (task-24).
    /// 한 백엔드가 None을 반환하면 폴백 체인을 순서대로 시도한다.
    fallbacks: BTreeMap<String, String>,
}

impl BackendPool {
    /// 빈 BackendPool을 생성한다.
    pub fn new() -> Self {
        Self {
            backends: BTreeMap::new(),
            semaphores: BTreeMap::new(),
            default_backend: None,
            routing: BTreeMap::new(),
            fallbacks: BTreeMap::new(),
        }
    }

    /// BackendConfig + system_prompts로 Backend를 빌드해 풀에 등록한다.
    ///
    /// - `config.protocol`에 따라 `Backend::Ollama` 또는 `Backend::OpenAI`를 빌드한다.
    /// - `system_prompts`: 화자별 system prompt 맵. 백엔드가 speaker로 조회.
    /// - 같은 이름으로 두 번 호출하면 덮어쓴다.
    /// - 백엔드 빌드 실패는 폴백(panic 없음) — reqwest::Client::new()로 대체된다.
    pub fn add(&mut self, config: BackendConfig, system_prompts: BTreeMap<PersonaId, String>) {
        let backend = match config.protocol {
            Protocol::Ollama => Backend::Ollama(OllamaBackend::new(
                config.model,
                config.endpoint,
                config.api_key,
                system_prompts,
                config.timeout,
                config.num_ctx,
                config.thinking,
            )),
            Protocol::OpenAI => Backend::OpenAI(OpenAIBackend::new(
                config.model,
                config.endpoint,
                config.api_key,
                system_prompts,
                config.timeout,
                config.max_tokens,
                config.thinking,
            )),
        };
        // 백엔드별 세마포어: config.max_concurrent 슬롯으로 생성한다.
        // add를 두 번 호출하면 세마포어도 덮어쓴다(slots 리셋).
        let sem = Semaphore::new(config.max_concurrent);
        self.semaphores.insert(config.name.clone(), sem);
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

    /// 백엔드 `name`의 폴백 백엔드 이름을 등록한다 (task-24).
    ///
    /// `name` 백엔드가 None을 반환하면 `fallback` 백엔드를 이어서 시도한다.
    /// 같은 이름으로 두 번 호출하면 덮어쓴다.
    /// 풀에 등록되지 않은 이름도 허용된다(등록 순서 무관).
    pub fn set_fallback(&mut self, name: impl Into<String>, fallback: impl Into<String>) {
        self.fallbacks.insert(name.into(), fallback.into());
    }

    /// speaker에 대한 폴백 체인(백엔드 이름 순서열)을 반환한다 (task-24, 순수 함수).
    ///
    /// - `resolve(speaker)`가 None이면 빈 Vec 반환.
    /// - 첫 원소는 primary 백엔드(resolve 결과), 이어서 fallbacks 링크를 따라간다.
    /// - 사이클/중복 안전: 방문한 이름은 즉시 중단(무한 루프 없음).
    ///
    /// 반환 예) primary="cloud", fallback["cloud"]="friend" → ["cloud", "friend"]
    /// 폴백 미설정 시 → ["cloud"]
    /// resolve None 시 → []
    ///
    /// 네트워크 접근 없음 → 단위 테스트에서 직접 검증 가능.
    pub fn fallback_chain(&self, speaker: &str) -> Vec<String> {
        // resolve가 None이면 체인 없음.
        let primary = match self.resolve(speaker) {
            Some(name) => name.to_string(),
            None => return vec![],
        };

        let mut chain = Vec::new();
        let mut visited = std::collections::BTreeSet::new();
        let mut current = primary;

        loop {
            // 이미 방문한 이름이면 사이클 — 중단.
            if !visited.insert(current.clone()) {
                break;
            }
            chain.push(current.clone());

            // 다음 폴백이 있으면 따라가고, 없으면 체인 완료.
            match self.fallbacks.get(&current) {
                Some(next) => current = next.clone(),
                None => break,
            }
        }

        chain
    }
}

/// 백엔드별 세마포어 캡이 적용된 병렬 작업 실행 헬퍼.
///
/// 각 job은 `(Arc<Semaphore>, Box<dyn FnOnce() -> T + Send>)` 쌍이다.
/// `thread::scope`로 job마다 스레드를 생성하고, 스레드 내에서 세마포어를 acquire한 뒤
/// 클로저를 실행한다. Permit drop으로 슬롯이 자동 반환된다.
///
/// 반환값은 **입력 순서**와 동일한 `Vec<T>`이다.
///
/// # 설계 결정
/// - `thread::scope`를 사용하므로 'static 바인딩이 불필요하다.
/// - `&Backend`가 `Send + Sync`이기 때문에 클로저에서 `&self` 경로를 빌릴 수 있다.
/// - rng를 소비하지 않는다 → 엔진 결정성 보존.
pub fn run_with_caps<T: Send>(
    jobs: Vec<(Arc<Semaphore>, Box<dyn FnOnce() -> T + Send>)>,
) -> Vec<T> {
    let n = jobs.len();
    // 결과를 인덱스 순서로 수집하기 위해 Option 슬롯을 미리 할당한다.
    let mut slots: Vec<Option<T>> = (0..n).map(|_| None).collect();

    std::thread::scope(|s| {
        // 각 슬롯에 대한 가변 참조를 하나씩 뽑아내기 위해 iter_mut로 분해한다.
        let handles: Vec<_> = slots
            .iter_mut()
            .zip(jobs)
            .map(|(slot, (sem, f))| {
                s.spawn(move || {
                    // 세마포어 acquire: 슬롯이 생길 때까지 블록.
                    let _permit = sem.acquire();
                    // permit을 보유한 상태에서 클로저를 실행한다.
                    let result = f();
                    // 슬롯에 결과를 저장한다.
                    *slot = Some(result);
                    // _permit drop → 슬롯 반환
                })
            })
            .collect();

        // 모든 스레드가 완료될 때까지 기다린다.
        for h in handles {
            // join 실패(스레드 패닉)는 None으로 처리한다(slot이 None인 채로 남음).
            let _ = h.join();
        }
    });

    // None 슬롯은 실패(스레드 패닉)를 의미한다. Option을 벗겨 반환하되,
    // 실패한 슬롯은 T의 기본값이 없으므로 컬렉션에서 제외한다.
    // 단 호출처(generate_batch)는 각 슬롯이 정확히 채워지길 기대한다.
    // 현재 구현에서 클로저 패닉은 없으므로 unwrap_or 경로는 도달하지 않는다.
    slots
        .into_iter()
        .map(|s| s.expect("run_with_caps: slot not filled — thread panicked"))
        .collect()
}

impl BackendPool {
    /// 여러 페르소나에 대해 병렬로 발화를 생성한다 (bench/비교 전용 경로).
    ///
    /// **라이브 틱 루프에서 절대 호출하지 않는다.** 이 경로는 rng를 소비하지 않으며
    /// 엔진 결정 경로(driver → PersonaRuntime::generate)와 분리된다(INV-1/INV-3 준수).
    ///
    /// # 인자
    /// - `jobs`: `(PersonaId, Vec<Event>)` 슬라이스. job마다 해당 페르소나의 발화를 생성한다.
    /// - `tick`: 현재 틱 번호(생성 컨텍스트 기록용).
    ///
    /// # 반환
    /// 입력 순서와 동일한 `Vec<(PersonaId, Option<String>)>`.
    /// 라우팅 대상 백엔드가 없거나 생성 실패이면 해당 슬롯은 `None`.
    ///
    /// # 동시성 보장
    /// 각 job은 라우팅된 백엔드의 `Arc<Semaphore>`를 acquire한 뒤 실행된다.
    /// 백엔드별 max_concurrent 슬롯을 초과하는 in-flight는 슬롯 반환까지 블록된다.
    /// 서로 다른 백엔드의 세마포어는 독립적으로 동작한다.
    ///
    /// # 구현 노트
    /// `thread::scope`를 직접 사용하므로 `&Backend` 빌림 수명이 `'static` 불요.
    /// `run_with_caps`는 `'static` 클로저 테스트 전용 헬퍼이므로 이 함수에서 직접 호출하지 않는다.
    pub fn generate_batch(
        &self,
        jobs: &[(PersonaId, Vec<Event>)],
        tick: u64,
    ) -> Vec<(PersonaId, Option<String>)> {
        let n = jobs.len();
        // 결과 슬롯: None = 아직 채워지지 않음(라우팅 실패 또는 초기값).
        let mut results: Vec<Option<String>> = (0..n).map(|_| None).collect();

        // 라우팅 해석: 유효 job만 수집 (task-24: 폴백 체인 포함).
        // chain: 이 job이 시도할 백엔드 이름 순서열.
        // backend_sems: chain 각 원소에 대응하는 (&Backend, Arc<Semaphore>) 쌍.
        //   체인 중 backends/semaphores에 없는 이름은 건너뛴다.
        // 이 시점에 &Backend는 &self 수명에 묶여 있으며 thread::scope 내에서 안전하게 공유된다.
        struct Job<'a> {
            original_idx: usize,
            /// 체인의 각 후보: (backend 참조, 세마포어). thread로 move될 때 Arc 복사.
            candidates: Vec<(&'a Backend, Arc<Semaphore>)>,
            speaker: PersonaId,
            history: &'a [Event],
        }

        let valid_jobs: Vec<Job<'_>> = jobs
            .iter()
            .enumerate()
            .filter_map(|(i, (speaker, history))| {
                // 폴백 체인을 먼저 구성한다.
                let chain = self.fallback_chain(speaker);
                if chain.is_empty() {
                    return None;
                }

                // 체인 내 각 이름에 대해 (backend, sem) 쌍을 수집한다.
                // backends 또는 semaphores에 없는 이름은 건너뛴다.
                let candidates: Vec<(&Backend, Arc<Semaphore>)> = chain
                    .iter()
                    .filter_map(|name| {
                        let backend = self.backends.get(name)?;
                        let sem = Arc::clone(self.semaphores.get(name)?);
                        Some((backend, sem))
                    })
                    .collect();

                if candidates.is_empty() {
                    return None;
                }

                Some(Job {
                    original_idx: i,
                    candidates,
                    speaker: speaker.clone(),
                    history: history.as_slice(),
                })
            })
            .collect();

        // thread::scope: 각 스레드가 &Backend를 빌릴 수 있다(scope가 수명 보장).
        // &Backend는 Send+Sync이므로 스코프 스레드에서 안전하게 공유된다.
        // 각 스레드는 폴백 체인을 순서대로 시도하고 (original_idx, Option<String>)을 반환한다.
        std::thread::scope(|s| {
            let thread_handles: Vec<_> = valid_jobs
                .iter()
                .map(|job| {
                    // Arc<Semaphore>는 clone으로 스레드에 move한다.
                    let candidates: Vec<(&Backend, Arc<Semaphore>)> = job
                        .candidates
                        .iter()
                        .map(|(b, sem)| (*b, Arc::clone(sem)))
                        .collect();
                    let speaker = job.speaker.clone();
                    let history = job.history;
                    let original_idx = job.original_idx;

                    s.spawn(move || {
                        // 폴백 체인: 첫 Some에서 멈춘다.
                        // 각 후보의 세마포어를 acquire → generate → permit drop(RAII).
                        // recall=None: generate_batch는 bench/비교 전용 경로 — 회상 불사용.
                        // system_prompt_override=None: 기존 동작(백엔드 내부 맵 조회) 유지.
                        for (backend, sem) in &candidates {
                            let _permit = sem.acquire();
                            let text = backend.generate(&speaker, history, tick, None, None);
                            // permit은 여기서 drop된다(다음 후보 시도 전 슬롯 반환).
                            drop(_permit);
                            if text.is_some() {
                                return (original_idx, text);
                            }
                            // None이면 체인의 다음 백엔드로.
                        }
                        (original_idx, None)
                    })
                })
                .collect();

            for h in thread_handles {
                if let Ok((idx, text)) = h.join() {
                    results[idx] = text;
                }
                // join 실패(스레드 패닉)이면 해당 슬롯은 None으로 남는다(panic 없음).
            }
        });

        // 결과를 입력 순서로 조립한다.
        jobs.iter()
            .enumerate()
            .map(|(i, (speaker, _))| (speaker.clone(), results[i].clone()))
            .collect()
    }
}

impl BackendPool {
    /// 워커 스레드에서 Arc<BackendPool>로 호출하기 위한 &self 생성 경로 (task-29).
    ///
    /// `fallback_chain(speaker)` 순서로 각 백엔드를 시도해 첫 `Some(text)`을 반환한다.
    /// 모든 백엔드가 None이면 None 반환(panic 없음). rng 불요 → 엔진 결정성 보존.
    ///
    /// - `recall`: 라이브 LiveSession 경로에서만 Some. generate_batch/PersonaRuntime은 None.
    pub fn generate_one(
        &self,
        speaker: &PersonaId,
        history: &[Event],
        tick: u64,
        recall: Option<&str>,
    ) -> Option<String> {
        // fallback_chain은 &self를 빌리므로 Vec<String>으로 복사해 borrow 충돌을 피한다.
        let chain = self.fallback_chain(speaker);

        for backend_name in &chain {
            if let Some(backend) = self.backends.get(backend_name) {
                if let Some(text) = backend.generate(speaker, history, tick, recall, None) {
                    return Some(text);
                }
                // None이면 체인의 다음 백엔드로. 폴백이 실제로 사용됐음을 로그.
                eprintln!(
                    "[tunaSalon] backend '{}' returned None, trying fallback (speaker={})",
                    backend_name, speaker
                );
            }
        }

        None
    }

    /// 지정된 backend_name의 백엔드에 직접 발화 텍스트 생성을 요청한다.
    ///
    /// `generate_one`(폴백 체인)과 달리 명시적으로 지정한 백엔드 하나만 시도한다.
    /// 폴백 체인은 적용하지 않는다 - backend를 명시 지정하는 게 목적이기 때문.
    ///
    /// - `backend_name`이 풀에 없으면 None 반환(panic 없음).
    /// - 세마포어: generate_one이 세마포어를 사용하지 않으므로 이 메서드도 사용하지 않는다.
    ///   (세마포어는 generate_batch의 병렬 경로에서만 사용한다.)
    /// - `system_prompt`: Some(p)이면 해당 백엔드의 내부 system_prompts 맵을 무시하고 p를 사용한다.
    ///   None이면 백엔드 내부 맵 조회(기존 동작).
    /// - rng를 소비하지 않는다 → 엔진 결정성 보존.
    /// - SECURITY: api_key는 백엔드 내부에서만 헤더에 첨부된다. 이 함수에서 노출하지 않는다.
    pub fn generate_on(
        &self,
        backend_name: &str,
        speaker: &PersonaId,
        history: &[Event],
        tick: u64,
        recall: Option<&str>,
        system_prompt: Option<&str>,
    ) -> Option<String> {
        let backend = self.backends.get(backend_name)?;
        backend.generate(speaker, history, tick, recall, system_prompt)
    }
}

impl PersonaRuntime for BackendPool {
    /// speaker를 폴백 체인 순서로 시도해 첫 Some을 반환한다 (task-24).
    ///
    /// - `generate_one`에 위임한다(task-29: 동작 동일, &mut 불필요하나 트레이트 서명 유지).
    /// - recall=None: driver/headless 경로는 회상 미주입 → 골든 바이트 동일 보존.
    /// - rng를 소비하지 않는다 → 엔진 결정성 보존(INV-1).
    fn generate(
        &mut self,
        speaker: &PersonaId,
        history: &[Event],
        tick: u64,
        _rng: &mut ChaCha8Rng,
    ) -> Option<String> {
        self.generate_one(speaker, history, tick, None)
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

    // -------------------------------------------------------------------------
    // task-27: Backend enum + OpenAI 통합 테스트 (네트워크 불필요)
    // -------------------------------------------------------------------------

    /// OpenAI 설정으로 add하면 Backend::OpenAI가 등록된다(resolve로 조회 가능).
    #[test]
    fn add_openai_config_registers_openai_backend() {
        let mut pool = BackendPool::new();
        let cfg = BackendConfig::new_openai(
            "friend",
            "qwen3.6-35b",
            "http://127.0.0.1:1", // 오프라인 - 라우팅만 검증
            None,
            1,
            Some(256),
            Duration::from_millis(1),
        );
        pool.add(cfg, BTreeMap::new());
        pool.set_default("friend");

        // resolve가 "friend"를 반환해야 한다
        assert_eq!(pool.resolve("any-speaker"), Some("friend"));
        assert_eq!(pool.len(), 1);
    }

    /// BackendConfig::new_openai의 protocol 필드가 OpenAI이다.
    #[test]
    fn new_openai_config_has_openai_protocol() {
        let cfg = BackendConfig::new_openai(
            "friend",
            "qwen3.6-35b",
            "http://yongseek.iptime.org:8008",
            None,
            1,
            None,
            Duration::from_secs(60),
        );
        assert_eq!(cfg.protocol, Protocol::OpenAI);
        assert_eq!(cfg.num_ctx, None); // OpenAI 프로토콜에 해당 없음
    }

    /// BackendConfig::new의 protocol 필드가 Ollama이다(기존 호출 보존).
    #[test]
    fn new_config_has_ollama_protocol_by_default() {
        let cfg = BackendConfig::new(
            "cloud",
            "gemma4:e4b",
            "https://api.ollama.ai",
            None,
            3,
            None,
            Duration::from_secs(30),
        );
        assert_eq!(cfg.protocol, Protocol::Ollama);
        assert_eq!(cfg.max_tokens, None);
    }

    /// mixed 풀(Ollama + OpenAI)에서 resolve가 올바르게 동작한다.
    #[test]
    fn mixed_pool_resolve_works_for_both_protocols() {
        let mut pool = BackendPool::new();

        // Ollama 백엔드 ("cloud")
        pool.add(make_config("cloud"), BTreeMap::new());
        // OpenAI 백엔드 ("friend")
        let cfg_openai = BackendConfig::new_openai(
            "friend",
            "qwen3.6-35b",
            "http://127.0.0.1:1",
            None,
            1,
            None,
            Duration::from_millis(1),
        );
        pool.add(cfg_openai, BTreeMap::new());
        pool.set_default("cloud");
        pool.add_route("anchor", "friend");

        assert_eq!(pool.len(), 2);
        // anchor는 friend(OpenAI)로 라우팅
        assert_eq!(pool.resolve("anchor"), Some("friend"));
        // 나머지는 cloud(Ollama)로 폴백
        assert_eq!(pool.resolve("chaos"), Some("cloud"));
    }

    /// mixed 풀에서 PersonaRuntime::generate가 오프라인이면 None(panic 없음).
    #[test]
    fn mixed_pool_generate_offline_returns_none_for_openai_backend() {
        use rand::SeedableRng;

        let mut pool = BackendPool::new();
        let cfg = BackendConfig::new_openai(
            "friend",
            "qwen3.6-35b",
            "http://127.0.0.1:1", // 오프라인
            None,
            1,
            None,
            Duration::from_millis(1),
        );
        pool.add(cfg, BTreeMap::new());
        pool.set_default("friend");

        let speaker = "anchor".to_string();
        let history = Vec::new();
        let mut rng = ChaCha8Rng::seed_from_u64(42);

        // 오프라인 → None, panic 없음
        let result = pool.generate(&speaker, &history, 0, &mut rng);
        assert_eq!(result, None);
    }

    // -------------------------------------------------------------------------
    // task-23: run_with_caps + generate_batch 단위 테스트 (네트워크 없음)
    // -------------------------------------------------------------------------

    /// run_with_caps: N개 job 결과가 입력 순서와 동일하게 반환된다.
    #[test]
    fn run_with_caps_preserves_input_order() {
        use crate::semaphore::Semaphore;
        use std::sync::Arc;

        let sem = Semaphore::new(4);
        let n = 8usize;
        let jobs: Vec<(Arc<Semaphore>, Box<dyn FnOnce() -> usize + Send>)> = (0..n)
            .map(|i| {
                let s = Arc::clone(&sem);
                let f: Box<dyn FnOnce() -> usize + Send> = Box::new(move || i);
                (s, f)
            })
            .collect();

        let results = run_with_caps(jobs);
        assert_eq!(results.len(), n);
        for (idx, val) in results.iter().enumerate() {
            assert_eq!(
                *val, idx,
                "결과 순서가 입력과 달라야 함: idx={idx}, val={val}"
            );
        }
    }

    /// run_with_caps: 두 독립 세마포어(cap 3 / cap 1)가 동시에 독립적으로 집행된다.
    ///
    /// - sem_a(cap=3): 8개 job → 피크 ≤ 3
    /// - sem_b(cap=1): 4개 job → 피크 ≤ 1
    /// 두 그룹을 한 번에 run_with_caps로 실행해 독립성을 검증한다.
    #[test]
    fn run_with_caps_two_semaphores_enforce_caps_independently() {
        use crate::semaphore::Semaphore;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;

        let cap_a = 3usize;
        let cap_b = 1usize;
        let sem_a = Semaphore::new(cap_a);
        let sem_b = Semaphore::new(cap_b);

        let current_a = Arc::new(AtomicUsize::new(0));
        let peak_a = Arc::new(AtomicUsize::new(0));
        let current_b = Arc::new(AtomicUsize::new(0));
        let peak_b = Arc::new(AtomicUsize::new(0));

        let mut jobs: Vec<(Arc<Semaphore>, Box<dyn FnOnce() -> () + Send>)> = Vec::new();

        // sem_a 그룹: 8개 job
        for _ in 0..8 {
            let s = Arc::clone(&sem_a);
            let cur = Arc::clone(&current_a);
            let pk = Arc::clone(&peak_a);
            let f: Box<dyn FnOnce() -> () + Send> = Box::new(move || {
                let prev = cur.fetch_add(1, Ordering::SeqCst);
                pk.fetch_max(prev + 1, Ordering::SeqCst);
                thread::yield_now();
                cur.fetch_sub(1, Ordering::SeqCst);
            });
            jobs.push((s, f));
        }

        // sem_b 그룹: 4개 job
        for _ in 0..4 {
            let s = Arc::clone(&sem_b);
            let cur = Arc::clone(&current_b);
            let pk = Arc::clone(&peak_b);
            let f: Box<dyn FnOnce() -> () + Send> = Box::new(move || {
                let prev = cur.fetch_add(1, Ordering::SeqCst);
                pk.fetch_max(prev + 1, Ordering::SeqCst);
                thread::yield_now();
                cur.fetch_sub(1, Ordering::SeqCst);
            });
            jobs.push((s, f));
        }

        run_with_caps(jobs);

        let obs_a = peak_a.load(Ordering::SeqCst);
        let obs_b = peak_b.load(Ordering::SeqCst);
        assert!(obs_a <= cap_a, "sem_a 피크({obs_a}) > cap_a({cap_a})");
        assert!(obs_b <= cap_b, "sem_b 피크({obs_b}) > cap_b({cap_b})");
    }

    /// generate_batch: 라우팅 실패(백엔드 없음)이면 해당 슬롯은 None을 반환한다(panic 없음).
    #[test]
    fn generate_batch_returns_none_for_unresolvable_jobs() {
        let pool = BackendPool::new(); // 백엔드 없음, default 없음

        let jobs = vec![
            ("friend".to_string(), vec![]),
            ("chaos".to_string(), vec![]),
        ];

        let results = pool.generate_batch(&jobs, 0);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, "friend");
        assert_eq!(results[0].1, None);
        assert_eq!(results[1].0, "chaos");
        assert_eq!(results[1].1, None);
    }

    /// generate_batch: 오프라인 백엔드(포트 1)이면 None, 입력 순서 보존, panic 없음.
    #[test]
    fn generate_batch_offline_returns_none_preserves_order() {
        let mut pool = BackendPool::new();
        pool.add(make_config("cloud"), BTreeMap::new()); // 포트 1 = 오프라인
        pool.set_default("cloud");

        let jobs = vec![
            ("friend".to_string(), vec![]),
            ("chaos".to_string(), vec![]),
            ("anchor".to_string(), vec![]),
        ];

        let results = pool.generate_batch(&jobs, 42);

        assert_eq!(results.len(), 3);
        // 오프라인이므로 모두 None
        for (i, (speaker, text)) in results.iter().enumerate() {
            assert_eq!(speaker, &jobs[i].0, "순서 불일치 idx={i}");
            assert_eq!(*text, None, "오프라인 백엔드는 None이어야 함(idx={i})");
        }
    }

    /// generate_batch: 세마포어가 add 시 생성된다(풀에 semaphore가 존재한다).
    #[test]
    fn semaphore_created_on_add() {
        let mut pool = BackendPool::new();
        pool.add(make_config("cloud"), BTreeMap::new());

        // 세마포어가 등록됐는지 간접 검증: generate_batch가 panic 없이 완료된다.
        let jobs: Vec<(PersonaId, Vec<crate::model::Event>)> = vec![];
        let results = pool.generate_batch(&jobs, 0);
        assert!(results.is_empty());
    }

    // -------------------------------------------------------------------------
    // task-24: fallback_chain + generate_batch 폴백 체인 단위 테스트 (네트워크 없음)
    // -------------------------------------------------------------------------

    /// fallback_chain: 폴백이 설정된 경우 [primary, fallback] 순서로 반환된다.
    #[test]
    fn fallback_chain_returns_primary_and_fallback() {
        let mut pool = BackendPool::new();
        pool.add(make_config("cloud"), BTreeMap::new());
        pool.add(make_config("friend"), BTreeMap::new());
        pool.set_default("cloud");
        pool.set_fallback("cloud", "friend");

        let chain = pool.fallback_chain("any-speaker");
        assert_eq!(chain, vec!["cloud".to_string(), "friend".to_string()]);
    }

    /// fallback_chain: 폴백이 없으면 [primary]만 반환된다.
    #[test]
    fn fallback_chain_returns_only_primary_when_no_fallback() {
        let mut pool = BackendPool::new();
        pool.add(make_config("cloud"), BTreeMap::new());
        pool.set_default("cloud");
        // set_fallback 없음

        let chain = pool.fallback_chain("any-speaker");
        assert_eq!(chain, vec!["cloud".to_string()]);
    }

    /// fallback_chain: resolve가 None(라우팅 없음)이면 빈 Vec.
    #[test]
    fn fallback_chain_returns_empty_when_unresolvable() {
        let pool = BackendPool::new(); // 백엔드 없음, default 없음

        let chain = pool.fallback_chain("any-speaker");
        assert!(chain.is_empty());
    }

    /// fallback_chain: 사이클(a→b, b→a) 시 체인이 유한하게 끝난다(무한 루프·panic 없음).
    #[test]
    fn fallback_chain_cycle_safe() {
        let mut pool = BackendPool::new();
        pool.add(make_config("a"), BTreeMap::new());
        pool.add(make_config("b"), BTreeMap::new());
        pool.set_default("a");
        // 사이클: a → b → a
        pool.set_fallback("a", "b");
        pool.set_fallback("b", "a");

        let chain = pool.fallback_chain("any-speaker");
        // 체인은 유한: ["a", "b"] (a가 재방문되는 순간 중단)
        assert_eq!(
            chain.len(),
            2,
            "사이클 시 체인 길이=2이어야 함: {:?}",
            chain
        );
        assert_eq!(chain[0], "a");
        assert_eq!(chain[1], "b");
    }

    /// generate_batch: 오프라인 primary + 오프라인 fallback → 모두 None, 입력 순서 보존, panic 없음.
    #[test]
    fn generate_batch_offline_primary_and_fallback_returns_none_preserves_order() {
        let mut pool = BackendPool::new();
        // 두 백엔드 모두 포트 1(연결 거부) — 빠른 타임아웃
        pool.add(make_config("cloud"), BTreeMap::new());
        pool.add(make_config("friend"), BTreeMap::new());
        pool.set_default("cloud");
        pool.set_fallback("cloud", "friend");

        let jobs = vec![
            ("alice".to_string(), vec![]),
            ("bob".to_string(), vec![]),
            ("carol".to_string(), vec![]),
        ];

        let results = pool.generate_batch(&jobs, 0);

        assert_eq!(results.len(), 3, "결과 개수가 입력과 동일해야 함");
        for (i, (speaker, text)) in results.iter().enumerate() {
            assert_eq!(speaker, &jobs[i].0, "순서 불일치 idx={i}");
            assert_eq!(
                *text, None,
                "오프라인 primary+fallback → None이어야 함(idx={i})"
            );
        }
    }

    // -------------------------------------------------------------------------
    // generate_on 단위 테스트 (네트워크 없음)
    // -------------------------------------------------------------------------

    /// generate_on: 존재하지 않는 backend_name이면 None을 반환한다(panic 없음).
    #[test]
    fn generate_on_returns_none_for_unknown_backend() {
        let pool = BackendPool::new(); // 빈 풀
        let speaker = "friend".to_string();
        let result = pool.generate_on("nonexistent", &speaker, &[], 0, None, None);
        assert_eq!(result, None, "알 수 없는 backend_name → None이어야 함");
    }

    /// generate_on: 오프라인 백엔드이면 None을 반환한다(panic 없음).
    #[test]
    fn generate_on_offline_backend_returns_none() {
        let mut pool = BackendPool::new();
        pool.add(make_config("cloud"), BTreeMap::new()); // 포트 1 = 오프라인

        let speaker = "friend".to_string();
        let result = pool.generate_on("cloud", &speaker, &[], 0, None, None);
        // 오프라인이므로 None, panic 없음
        assert_eq!(result, None, "오프라인 백엔드 → None이어야 함");
    }

    /// generate_on: 풀백 체인을 따르지 않는다.
    /// "cloud"만 등록하고 "friend"로 요청하면 None(폴백 없음).
    #[test]
    fn generate_on_does_not_follow_fallback_chain() {
        let mut pool = BackendPool::new();
        pool.add(make_config("cloud"), BTreeMap::new());
        pool.set_default("cloud");
        // "friend"는 풀에 없음 — generate_on은 폴백 체인을 무시하고 "friend"만 시도
        let speaker = "any".to_string();
        let result = pool.generate_on("friend", &speaker, &[], 0, None, None);
        assert_eq!(result, None, "명시 backend가 없으면 None(폴백 체인 미적용)");
    }
}
