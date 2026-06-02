# Plans

Plan document index. Register new plans here.

| slug | 상태 | 버전 | 요약 |
|------|------|------|------|
| [salon-engine-v1](salon-engine-v1.md) | in_progress | v1 | Salon 대화 흐름 엔진 실행 플랜. **v0.1 구현 완료**(Rust, 27 tests, 스모크 게이트 green) ~ v0.6(MetaController) 로드맵 |
| [salon-engine-v2](salon-engine-v2.md) | done | v0.2 | **구현 완료**(50 tests, 게이트 green). 교차 자극 α(room preset + persona modifier) + FSM 전이, spectral radius < 1, α=0 골든 보존 |
| [salon-engine-v3](salon-engine-v3.md) | done | v0.3 | **구현 완료**(70 tests, 게이트 green). PersonaRuntime(Fake/Ollama), Event.content, 내용 기반 RRF(관심도·잔향), persona collapse 도구. 화자 선택은 엔진(결정적), 생성만 LLM. 기본 LLM off + 골든 보존 |
| [salon-engine-v4](salon-engine-v4.md) | in_progress | v0.4 | **설계 완료, 구현 대기**. 이종 백엔드 풀(Cloud 동시성 3 + 지인서버 qwen3.6:32b 동시성 2) + 페르소나별 라우팅(mixed-model 방) + 백엔드별 세마포어/큐/타임아웃 폴백. 동시성은 비교/벤치 전용, 라이브 순차. async 미도입(blocking+thread::scope). num_ctx 백엔드별 Option. task 21~26 |
