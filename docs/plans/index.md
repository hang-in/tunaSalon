# Plans

Plan document index. Register new plans here.

| slug | 상태 | 버전 | 요약 |
|------|------|------|------|
| [salon-engine-v9](salon-engine-v9.md) | done | v0.9 | **구현 완료**(222 tests/feature 230, smoke_v9+smoke_chat+recall_eval). friend engine 심화 1단계: Stage 0(Lindera 한국어 형태소) + Stage 1(SQLite 영속 + FTS5 BM25 회상, `~/.local/share/tunaSalon/memory.db`). seCall 검색코어 lift. `friend-engine` feature 뒤(기본 off→골든·기본빌드 lean), 회상 라이브 전용→골든 보존. 채팅 생동감(3-way config+/topic) 동반. BGE-M3/usearch/hybrid는 v0.10. task 43~46 |
| [salon-web-frontend](salon-web-frontend.md) | planned | web(병렬) | **파킹(착수 전, 결정 기록)**. "프로덕션 레벨 앱" 목표로 채팅 UI를 web으로. Rust 엔진 그대로 + axum **WebSocket** 새 sink(엔진 push↔사람 입력, blocking 엔진↔async 채널 브리지) + 정적 HTML/CSS/JS 1장. TUI는 디버그 sink로 강등(유지). golden/headless 무손상, 키는 서버에만(WASM-only 불가). feature flag로 기본 빌드 무영향 |
| [salon-engine-v1](salon-engine-v1.md) | in_progress | v1 | Salon 대화 흐름 엔진 실행 플랜. **v0.1 구현 완료**(Rust, 27 tests, 스모크 게이트 green) ~ v0.6(MetaController) 로드맵 |
| [salon-engine-v2](salon-engine-v2.md) | done | v0.2 | **구현 완료**(50 tests, 게이트 green). 교차 자극 α(room preset + persona modifier) + FSM 전이, spectral radius < 1, α=0 골든 보존 |
| [salon-engine-v3](salon-engine-v3.md) | done | v0.3 | **구현 완료**(70 tests, 게이트 green). PersonaRuntime(Fake/Ollama), Event.content, 내용 기반 RRF(관심도·잔향), persona collapse 도구. 화자 선택은 엔진(결정적), 생성만 LLM. 기본 LLM off + 골든 보존 |
| [salon-engine-v8](salon-engine-v8.md) | done | v0.8 | **구현 완료**(210 tests, 스모크 8종 + recall_eval). friend engine(장기기억) 첫 증분: 참여 기반 인메모리 기억(같은 방 캐릭터만 회상) + 키워드 회상(`memory.rs`) + v0.3 회상 슬롯 주입(라이브 경로만→골든 보존) + SSOT 회상 평가 하네스. BGE-M3/SQLite/망각은 이후. task 39~42 |
| [salon-engine-v7](salon-engine-v7.md) | done | v0.7 | **구현 완료**(184 tests, 스모크 7종). MetaController: 수렴↑→mu_scale(=μ곱)↓로 방 식힘(`meta.rs`, driver/live). 약한 게인+floor 0.4. content 게이팅(flow None→1.0→골든 보존). 채팅 "식힘 ×" 표시. 원래 로드맵 엔진층 마지막. task 36~38 |
| [salon-engine-v6](salon-engine-v6.md) | done | v0.6 | **구현 완료, 라이브 검증**(168 tests, 스모크 6종). FlowMeter: 대화 수렴/발산 토큰 중복 근사(`flow.rs`), record + 채팅 사이드바 게이지, chat_demo flow 출력. 관찰만(엔진 피드백 금지=v0.7). content 게이팅으로 골든 보존. BGE-M3는 이후. task 33~35 |
| [salon-engine-v5](salon-engine-v5.md) | done | v0.5 | **구현 완료, 라이브 검증**(148 tests, 스모크 5종). 사람 참여 채팅방: HumanChannel(design §5) + LiveSession(논블로킹 생성) + 채팅 TUI(persona-ui §5) + `--chat`/chat_demo. 사람이 cloud 페르소나와 실제 대화(chat_demo 전사 확인). FlowMeter→v0.6. task 28~32 |
| [salon-engine-v4](salon-engine-v4.md) | done | v0.4 | **구현 완료**(125 tests, 스모크 게이트 4종 green, 라이브 검증). 이종 백엔드 풀(cloud `gemma4:31b-cloud` 동시성 3 + 지인서버 vLLM `qwen3.6-35b-fast` 동시성 1) + 페르소나별 라우팅(mixed-model 방 라이브 작동) + 폴백 체인. Backend enum(Ollama\|OpenAI). 동시성은 비교/벤치 전용·라이브 순차. async 미도입(blocking+thread::scope). 로컬 ollama 금지(가드). task 21·22·27·23·24·25·26 |
