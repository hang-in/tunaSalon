# Plans

Plan document index. Register new plans here.

| slug | 상태 | 버전 | 요약 |
|------|------|------|------|
| [salon-engine-v1](salon-engine-v1.md) | in_progress | v1 | Salon 대화 흐름 엔진 실행 플랜. **v0.1 구현 완료**(Rust, 27 tests, 스모크 게이트 green) ~ v0.6(MetaController) 로드맵 |
| [salon-engine-v2](salon-engine-v2.md) | done | v0.2 | **구현 완료**(50 tests, 게이트 green). 교차 자극 α(room preset + persona modifier) + FSM 전이, spectral radius < 1, α=0 골든 보존 |
| [salon-engine-v3](salon-engine-v3.md) | done | v0.3 | **구현 완료**(70 tests, 게이트 green). PersonaRuntime(Fake/Ollama), Event.content, 내용 기반 RRF(관심도·잔향), persona collapse 도구. 화자 선택은 엔진(결정적), 생성만 LLM. 기본 LLM off + 골든 보존 |
| [salon-engine-v6](salon-engine-v6.md) | done | v0.6 | **구현 완료, 라이브 검증**(168 tests, 스모크 6종). FlowMeter: 대화 수렴/발산 토큰 중복 근사(`flow.rs`), record + 채팅 사이드바 게이지, chat_demo flow 출력. 관찰만(엔진 피드백 금지=v0.7). content 게이팅으로 골든 보존. BGE-M3는 이후. task 33~35 |
| [salon-engine-v5](salon-engine-v5.md) | done | v0.5 | **구현 완료, 라이브 검증**(148 tests, 스모크 5종). 사람 참여 채팅방: HumanChannel(design §5) + LiveSession(논블로킹 생성) + 채팅 TUI(persona-ui §5) + `--chat`/chat_demo. 사람이 cloud 페르소나와 실제 대화(chat_demo 전사 확인). FlowMeter→v0.6. task 28~32 |
| [salon-engine-v4](salon-engine-v4.md) | done | v0.4 | **구현 완료**(125 tests, 스모크 게이트 4종 green, 라이브 검증). 이종 백엔드 풀(cloud `gemma4:31b-cloud` 동시성 3 + 지인서버 vLLM `qwen3.6-35b-fast` 동시성 1) + 페르소나별 라우팅(mixed-model 방 라이브 작동) + 폴백 체인. Backend enum(Ollama\|OpenAI). 동시성은 비교/벤치 전용·라이브 순차. async 미도입(blocking+thread::scope). 로컬 ollama 금지(가드). task 21·22·27·23·24·25·26 |
