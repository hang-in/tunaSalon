---
title: 다음 세션 첫 대화 복붙 프롬프트 (web 제품 트랙 라이브 튜닝 + 영속 4축)
type: reference
status: active
updated_at: 2026-06-03
---

# 다음 세션 킥오프 프롬프트

아래 블록을 새 세션 첫 메시지로 복붙하세요. (이번 세션에서 web 제품 트랙을 대거 구현: 동적 persona 초대 + 단일방 영속 + 형태소 flow + 활기 게이지 + 4축 배지 + pace 조절 + LLM fast/no-think + base_rate 튜닝까지. `--web` 라이브로 사용자가 직접 테스트하며 버그를 잡는 중.)

---

```
tunaSalon 이어서 작업한다. 먼저 CLAUDE.md + 메모리(web-ux-flow, friend-server-vllm, ort-embedding-viable) + docs/plans/salon-web-ux.md + git log --oneline -30 을 읽고 현황을 파악해.

현재 상태(web 제품 트랙, 라이브 검증 중):
- 동적 persona 초대: LiveSession add/remove_persona + persona_meta(backend/system_prompt/modifier/axes) + pool.generate_on + 케미 alpha 동적 재계산(recompute_alpha, spectral radius 정규화). web invite/remove 프레임. persona_kit assemble(4축 조립) + 인디언식 자동 이름.
- 단일방 영속: RoomStore(rooms.db, web feature, friend engine memory.db와 독립) 저장/복원. 단 system_prompt 텍스트만 저장 -> 코드 바꾸면 기존 방 미반영(매번 rooms.db 비움) + 4축 미저장이라 복원 시 배지 사라짐.
- 형태소 flow: morphology feature(Lindera, friend-engine/web 의존). 한국어 어간 통일로 흐름/냉각 게이지가 한국어에서 작동(단 발산 대화는 수렴도 낮음=정상).
- 활기 게이지(liveliness): 최근 발화 빈도. 흐름/냉각 위에 추가.
- 4축 배지(채팅), pace 런타임 조절(기본 6s), 입장/퇴장 알림, 화자 이름/색 동적(message.name + id해시), pending 마지막 그룹만, 닉네임 공백 제거, persona 자기 이름+대화 가드레일+한국어 프롬프트.
- LLM: friend=qwen3.6-35b-fast + thinking off(reasoning ~70s 제거), cloud(gemma) thinking off. vllm-swap 주의(모델 전환 첫 발화 ~2.5분, timeout 180s).
- base_rate 1차 튜닝: 동적 persona가 역할/MBTI 편차로 한 명만 발화하던 것 -> base_rate 선형 압축(0.55+raw*0.30) + reactivity 하한 0.4. 모든 초대 persona가 theta 근처로.

검증: 골든 5/5, default ~226 / web ~233 / friend-engine ~277, smoke green.

다음 우선순위:
1. **영속 4축 저장 + 복원 재assemble**(최우선). rooms.db에 4축(blood/mbti/zodiac/role) 저장하고 복원 시 assemble로 재조립 -> (a)코드(assemble) 변경이 기존 방에 자동 반영(rooms.db 매번 비우는 번거로움 해결) (b)복원해도 4축 배지 유지. PersonaMeta.axes는 이미 있음(roomstore 스키마 + save/load에 4축 추가).
2. base_rate 라이브 튜닝 계속: 아직 한 명 독점이면 압축 더(계수 조정), 너무 다 떠들면 편차 살림. persona_kit assemble base_rate 공식(0.55/0.30) + reactivity 하한.
3. 흐름 게이지: 발산 대화에서 정적(정상). 활기 게이지로 보완했으나, 흐름을 발산↔수렴 양방향 표시로 개선 검토.
4. 멀티룸(방 선택/생성/전환, WS room id, LiveSession 다중), 본인 프로필 + 프리셋 8개.
5. 웹서치(tool use): persona가 검색 결과를 대화에 끌어오기(별도 큰 트랙).

확정 결정(다시 묻지 말 것):
- 영속 = 서버 SQLite(rooms.db, web 독립). 모델 = friend qwen3.6-35b-fast / cloud gemma, 둘 다 thinking off. 자동배분 cloud 1/qwen 2.
- 동적 초대 설계 = 방향 B(pool은 Arc 고정 컨테이너, LiveSession persona_meta로 라우팅/프롬프트 전권 + generate_on).
- 라이브 튜닝은 사용자가 --web으로 직접 테스트하며 피드백(엔진 손잡이: base_rate/reactivity/target_rho/theta/pace).

진행: 구현은 Sonnet 서브에이전트(Agent tool, model sonnet)에 위임, Claude(Opus)가 스펙·리뷰·커밋. codex 비사용. 매 변경 후 골든 5종 + default/web 테스트 + (필요시) 프런트 pnpm/vite build 검증. 최종 답변 한국어, em-dash 금지.

주의: 지난 세션 --web 서버가 백그라운드로 떠 있을 수 있다(없으면 cargo run --features web -- --web). 코드(persona_kit/flow/web) 바꾸면 서버 재시작 + rooms.db 비워야 반영(4축 영속 전까지).
```

---

## 참고 (핸드오프 보강)

- **영속 복원 한계(최우선 후속의 이유)**: `roomstore.rs` save가 `persona_meta`의 backend/system_prompt/modifier만 저장(axes 무시), load는 axes=None. 그래서 (a)assemble 코드 변경이 복원된 방에 안 먹힘 (b)복원 persona는 4축 배지 없음. 해결 = roomstore 스키마에 4축 컬럼 + save/load + main 복원 분기에서 `assemble(blood,mbti,zodiac,role,name)`로 system_prompt 재생성(이름은 저장값 유지).
- **엔진 라이브 튜닝 손잡이**: base_rate(persona_kit assemble 0.55+raw*0.30), reactivity/provocativeness 하한(0.4), target_rho(web=Pub 0.40, with_target_rho), theta(chat_config 0.60), pace(기본 6s + UI). 한 명 독점 vs 균형 vs 정신없음 사이 조정.
- **vllm-swap**: 지인서버는 한 모델만 GPU. friend(qwen3.6-35b-fast) 첫 발화 ~2.5분(swap-in). cloud(gemma, ollama localhost)는 swap 무관 빠름. thinking=false면 즉답 0.3s.
- **흐름 vs 활기**: 흐름/냉각=수렴도(발산 대화는 0/100% 정적, 정상). 활기=발화 빈도(들썩임). 둘 다 사이드바 "방 상태" 카드.
- **plan**: `docs/plans/salon-web-ux.md`(1단계 동적초대 + 2단계 영속 완료, 3단계+ 미작성). README는 web 트랙 반영됨(badge 271).
