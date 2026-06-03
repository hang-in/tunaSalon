---
title: 다음 세션 첫 대화 복붙 프롬프트 (web 제품 UX 1단계 = 동적 persona 초대)
type: reference
status: active
updated_at: 2026-06-03
---

# 다음 세션 킥오프 프롬프트

아래 블록을 새 세션 첫 메시지로 복붙하세요. (이번 세션에서 v0.10 마감 + web P1~P3 + 일시정지 + thinking + persona_kit/인디언 이름까지 완료. 다음은 web 제품 UX 큰그림의 1단계 = 동적 persona 초대, 엔진 대수술이라 깨끗한 컨텍스트에서.)

---

```
tunaSalon 이어서 작업한다. 먼저 CLAUDE.md(핸드오프) + 메모리(web-ux-flow, web-frontend-track, friend-server-vllm) + docs/plans/salon-web-frontend.md + git log --oneline -25 를 읽고 현황을 파악해.

현재 완료: v0.1~v0.10(엔진 + friend engine 의미검색). web 프런트(P1~P3): axum WebSocket + Kimi React(web/) 배선, 아바타 하단 얇은 λ바, 이름 옆 모델 표시, 방 상태 통합 카드, 토픽 한글 IME 버그 fix, WS 자동 재연결+연결상태, 일시정지/재개(paused). LLM thinking opt-in(BackendConfig.thinking; qwen3.6-35b reasoning ~70s/발화, gemma ~5s). persona_kit(40조각 런타임 조립 assemble + /invite parse + 인디언식 이름 indian_name: 혈액형->형용사 + MBTI->자연/동물 + 별자리->어미, 예 "평화로운태양아래에서", 사람·페르소나 공용, assemble(name="")이면 자동). default 250+ tests, 골든 5/5 무손상.

다음 = web 제품 UX 큰그림(메모리 [[web-ux-flow]]: 방 선택/생성 -> 본인 프로필 -> 참가자 초대 -> 채팅 -> 나가기/탭닫힘 시 저장+이어가기). 그 1단계 = **동적 persona 초대**(엔진 대수술).

확정 결정(다시 묻지 말 것):
- 영속 = 서버측 SQLite(방 이어가기엔 서버 필수). 프로필/프리셋도 서버.
- 초대 persona 모델 배정 = 자동(cloud 1 / qwen 2).
- 이름 = persona_kit indian_name 자동 생성(임의 수정 불가, 혈/MBTI/별은 수정 가능).
- 동적 초대 설계 = **방향 B**(pool 가변화 회피): pool은 Arc<BackendPool>로 워커 공유라 런타임 가변 불가. LiveSession이 persona_meta[id -> (backend_name, system_prompt, modifier)]로 라우팅/프롬프트 전권 관리, 생성 job에 prompt/backend를 실어 pool.generate_on(backend, history, prompt) 호출(pool은 cloud/qwen 고정 컨테이너). add_persona(assemble 결과 + 자동 모델배분) / remove_persona가 state.intensities/excitations + CouplingMatrix(α, 신규 쌍 기본 0) + store.join 을 동적 갱신. 기존 고정 3명도 persona_meta로 통일.

진행: 먼저 plan 문서(salon-web-frontend.md 확장 또는 새 plan)로 설계 B + 단계를 박은 뒤 단계별로 Sonnet에 위임:
1) 엔진: LiveSession add/remove_persona + persona_meta + pool.generate_on + state/α 동적. live.rs/pool.rs/model.rs(EngineState, CouplingMatrix.values: BTreeMap<(PersonaId,PersonaId),f64>) 구조 보고 설계. 단위 테스트(add/remove 후 state 일관, 결정성, store.join, 기존 3명 동작 보존). 골든 무손상(LiveSession 전용, driver/headless 불침투).
2) web.rs: ClientFrame Invite{blood,mbti,zodiac}/Remove{id} -> EngineCmd -> LiveSession. participants(model 포함) 동적 반영. types/index.ts 계약 일관.
3) 프런트: 빈 방 초대 UI(혈/MBTI/별 드롭다운 -> persona_kit 이름 미리보기 -> 추가, 최대 3명) + remove.

이후(다음 단계들): 영속(서버 SQLite: 방 메시지+참가자 저장/복원, 나가기·탭닫힘 시 저장) -> 멀티룸(방 선택/생성/전환) -> 본인 프로필 + 프리셋 8개 + 저장된 프리셋.

검증: 골든 5종 + default/friend-engine/friend-engine-semantic 테스트 green. web feature 빌드(cargo build --features web) + 프런트 pnpm build(이 셸에서 pnpm 안 되면 web/node_modules/.bin/tsc·vite 직접). 실 LLM은 mixed_bench(SALON_THINK 토글)/--web.

구현은 Sonnet 서브에이전트(Agent tool, model sonnet)에 위임, Claude(Opus)가 스펙·리뷰·커밋. codex 비사용. pnpm 사용(npm 아님). 최종 답변 한국어, em-dash 금지.
```

---

## 참고 (핸드오프 보강)

- **엔진 구조(동적 초대 핵심)**: `EngineState{ intensities: BTreeMap<PersonaId,f64>, excitations: BTreeMap<.., f64>, history, last_speaker, rng_seed }`. `CouplingMatrix{ values: BTreeMap<(PersonaId,PersonaId), f64> }`(신규 persona 쌍은 기본 0 = 자극 없음으로 시작, 이후 modifier 기반 튜닝 가능). `pool`: backends/semaphores 고정 + routing/fallbacks(BTreeMap). `pool.add_route`는 &mut라 Arc로는 호출 불가 -> 방향 B로 우회.
- **persona_kit 재사용**: `assemble(role, mbti, blood, zodiac, name) -> AssembledPersona{ persona, system_prompt, modifier, visual }`, `indian_name(mbti, blood, zodiac)`, `parse_invite("entp b sag critic 입털이")`. 4축 FromStr(대소문자 무관). 이미 28 tests green.
- **모델 자동 배분(cloud 1/qwen 2)**: 현재 라우팅과 일치(summarizer=cloud gemma, friend/chaos=qwen). 동적 초대 시 backend를 cloud 1명 채우고 나머지 qwen으로(cap 설정 cloud 1/qwen 2와 정합).
- **thinking 주의**: qwen3.6-35b reasoning은 발화당 ~70초([[friend-server-vllm]]). 동적 초대로 qwen persona가 늘면 그만큼 느려짐 - 페이싱 체감 시 gemma 비중/ tick 주기 재검토 여지.
- **데이터 계약**: web.rs ServerFrame/ClientFrame <-> web/src/types/index.ts 항상 양쪽 일관. participants에 model: Option<String> 이미 있음.
- **골든 베이스라인**: 5종 /tmp/salon_golden/(레포 밖). `cargo build` 후 명시적 순차 실행(zsh는 feature 인자 따옴표 변수 워드스플릿 안 됨 - 따로 호출).
- **비주얼층(픽셀아트)**: 외부 이미지 에셋 대기로 보류. persona_kit VisualHint{palette, prop} 슬롯만 있음(렌더 X). 동적 초대/프로필에서 이름·게이지까지만, 캐릭터 스프라이트는 에셋 도착 후.
