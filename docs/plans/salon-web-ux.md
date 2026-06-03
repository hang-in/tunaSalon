---
title: Salon web 제품 UX 큰그림 - 멀티룸·동적 persona·영속 (1단계 = 동적 persona 초대, 방향 B)
type: plan
status: in_progress
priority: P1
updated_at: 2026-06-03
owner: shared
summary: web 프런트(P1~P3, salon-web-frontend.md)로 채팅방을 띄웠다. 이제 제품 UX 큰그림을 얹는다 - 방 선택/생성 -> 본인 프로필 -> 참가자 초대 -> 채팅 -> 나가기/탭닫힘 시 저장+이어가기(메모리 [[web-ux-flow]]). 그 1단계 = **동적 persona 초대**: 현재 고정 3명을 런타임 add/remove로. 엔진 대수술이라 **방향 B**(pool은 Arc 고정 컨테이너, LiveSession이 persona_meta로 라우팅/프롬프트 전권)로 우회한다. persona_kit(40조각 조립 + 인디언식 이름)을 재사용. 이후 단계 = 영속(서버 SQLite) -> 멀티룸 -> 프로필/프리셋.
design_ref: ../reference/salon-engine-design.md
ui_ref: ../temp/salon-persona-ui.md
web_track_ref: salon-web-frontend.md
flow_ref: ../temp/web-ux-flow.md
---

# Salon - web 제품 UX 큰그림 (멀티룸·동적 persona·영속)

> 트랙 구분: `salon-web-frontend.md`는 "엔진을 web sink로 옮기는" 전송/배선 트랙(P1~P3 완료). 이 문서는 그 위에 올리는 **제품 UX 큰그림**(멀티룸/동적 persona/프로필/영속) 트랙이다. 엔진 버전 라인(v0.x)을 점유하지 않는다.

## 0. Context / 동기

web 프런트(axum WebSocket + Kimi React)로 채팅방이 떴고, 일시정지/모델표시/재연결까지 됐다. 하지만 방은 **고정 3명**(summarizer/friend/chaos)으로 시작해 사용자가 참가자를 못 고른다. UX 큰그림(메모리 [[web-ux-flow]], 2026-06-03 확정):

```
방 선택/생성 -> 본인 프로필(혈액형/MBTI/별자리 + 자동 이름)
            -> 참가자 초대(조립/프리셋 8개/저장, 최대 3명)
            -> 채팅 -> 일시정지/나가기/탭닫힘 시 방 저장 + 이어가기
```

이 문서는 그 큰그림의 **1단계 = 동적 persona 초대**를 상세 설계하고, 이후 단계(영속 -> 멀티룸 -> 프로필/프리셋)를 phase로 박는다. 1단계가 가장 무겁다(엔진이 런타임에 참여자 수가 바뀌는 걸 못 하던 구조라 대수술).

### 왜 대수술인가 (현재 구조의 막힘)

`Explore`로 확인한 사실:
- `LiveSession.personas: Vec<Persona>`, `state.intensities/excitations: BTreeMap`, `config.alpha: CouplingMatrix`(LiveSession이 config를 **소유** -> `&mut self`면 갱신 가능), `speaker_labels: BTreeSet`, `store`(`join` 있음, 역함수 없음) - **여기까지는 동적 가능**.
- **막힘 1**: persona별 backend 라우팅은 `pool.routing`(BTreeMap), persona별 system_prompt는 `OllamaBackend`/`OpenAIBackend` **내부 맵**(`system_prompts: BTreeMap<PersonaId,String>`)에 갇혀 있다. `pool`은 `Arc<BackendPool>`로 워커 스레드와 공유돼 런타임 `&mut` 불가(`add`/`add_route`는 `&mut self`).
- **막힘 2**: 따라서 새 persona를 추가해도 그 persona의 backend/prompt를 pool에 등록할 길이 없다.

### 방향 B (확정, 2026-06-03)

pool을 가변화하지 않는다. 대신:
- pool은 **고정 컨테이너**: cloud/qwen 두 backend + 세마포어(cloud cap 1, qwen cap 2)만 보유. 라우팅/프롬프트 책임을 뺀다.
- `LiveSession`이 `persona_meta: BTreeMap<PersonaId, PersonaMeta>`(= backend 이름 + system_prompt + modifier)로 **라우팅/프롬프트 전권**을 가진다.
- 생성 job에 backend 이름과 system_prompt를 실어 `pool.generate_on(backend, speaker, history, tick, recall, system_prompt)`를 호출한다(backend는 컨테이너에서 고르기만).
- system_prompt를 backend 내부 맵이 아니라 **호출 인자로** 주입하려면 `ollama.rs`/`openai.rs`의 generate에 override 파라미터를 추가한다(None이면 기존 내부 맵 조회 = 무회귀).
- `add_persona`(assemble 결과 + 자동 모델배분) / `remove_persona`가 `intensities`/`excitations` + `config.alpha`(신규 쌍 기본 0) + `store.join`/`leave` + `speaker_labels` + `persona_meta`를 동적 갱신.
- 기존 고정 3명도 `persona_meta` 경유로 재구성(동작 보존).

## 1. 핵심 결정 (Architect)

- **방향 B** (위). pool은 `Arc<BackendPool>` 그대로, 런타임 가변 없음.
- **system_prompt 주입 = 호출 인자**. `Backend::generate`와 `OllamaBackend::generate_shared`/`OpenAIBackend::generate`에 `system_prompt_override: Option<&str>` 추가. `Some`이면 그 prompt 사용, `None`이면 기존 `system_prompts` 맵을 speaker로 조회(기존 호출처 전부 None -> 무회귀).
- **모델 자동 배분 = cloud 1 / qwen 2** (현 라우팅·세마포어 cap과 정합). `add_persona`가 현재 backend별 인원을 보고 cloud가 비면 cloud, 아니면 qwen에 배정(결정적 규칙).
- **이름 = persona_kit `indian_name` 자동** (혈액형 형용사 + MBTI 자연/동물 + 별자리 어미). 임의 수정 불가, 혈/MBTI/별은 수정 가능.
- **기존 3명도 persona_meta로 통일**. `with_store`(또는 web 생성 경로)에서 3명의 (backend, system_prompt, modifier)를 persona_meta에 채운다. 이후 tick의 job 디스패치는 persona_meta만 본다.
- **alpha 갱신**: LiveSession이 `config`를 소유하므로 `&mut self.config.alpha.values`로 직접 갱신. 신규 persona 쌍 (new, j)/(j, new)는 기본 0(자극 없음)으로 시작, 이후 modifier 기반 튜닝 여지(이 단계는 0 고정).
- **store.leave 추가**: `participation` 맵에서 persona 제거(remove 시). 기존 사건(events) 자체는 보존(회상 격리는 participation으로만).
- **결정성(라이브 단위 테스트용)**: add/remove 후 `intensities` 키셋 = personas 키셋 = persona_meta 키셋 = store participation(해당 방)이 항상 일치. BTreeMap/BTreeSet 순서 일관. 라이브 경로라 골든과 무관(LLM 비결정), 단 단위 테스트는 상태 일관성을 결정적으로 검증.
- **시작점**: 이 1단계는 **기존 3명 유지 + add/remove 가능**까지. "빈 방에서 초대로 채우기"는 web UX 후속(멀티룸/방 생성 단계)에서. 단 add/remove가 0~N명을 견디게 설계(빈 방 대비).

## 2. Invariants

| ID | 내용 |
|----|------|
| INV-1 | **골든 무손상**: 동적 초대는 LiveSession 라이브 경로 전용. driver/headless 불침투 -> 골든 5종 바이트 동일. system_prompt override는 opt-in(None=기존 동작)이라 기존 LLM 경로도 무회귀 |
| INV-2 | **pool 무가변**: `pool`은 `Arc<BackendPool>` 그대로. `add_persona`가 pool을 `&mut`로 건드리지 않음(`generate_on`이 backend를 고르기만). 워커 스레드 공유 안전 유지 |
| INV-3 | **상태 일관성**: 임의의 add/remove 시퀀스 후 personas / state.intensities / persona_meta / store participation(방)의 persona 키셋이 항상 일치. excitations/alpha도 제거된 persona 흔적 없음 |
| INV-4 | **기존 3명 동작 보존**: persona_meta 통일 후에도 3명방 라이브 동작(라우팅 cloud1/qwen2, system_prompt, 회상)이 현재와 동일. 단위/스모크로 확인 |
| INV-5 | **키는 서버에만**(web INV-3 계승): system_prompt/모델명은 프레임에 가도 api_key는 절대 노출 안 함 |
| INV-6 | v0.10 테스트 카운트(default 226 / friend-engine 235 / semantic 263) + 스모크 유지(+ 신규 add/remove 단위 테스트) |

## 3. Subtasks

> **진행(2026-06-03): 1단계 task A~E 전부 구현·단위검증 완료, 커밋됨**(A `d80075b` / C `92b51bd` / B `0bfbe8d` / D `9a43913` / E `5146b6a`). 검증: 골든 5/5 바이트 동일, default/web/friend-engine/friend-engine-semantic 빌드 green, default·friend-engine 테스트 0 failed, 프런트 `tsc -b`/`vite build` green. **라이브 검증(`--web`으로 브라우저에서 초대/퇴장 실제 동작 + qwen thinking 페이싱)은 사용자 권장**. 이후 단계(영속->멀티룸->프로필)는 §7.

| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| A | backend prompt override + generate_on | `ollama.rs`/`openai.rs` generate에 `system_prompt_override: Option<&str>` 추가(None=기존 맵). `Backend::generate` 전달. `BackendPool::generate_on(backend_name, speaker, history, tick, recall, system_prompt)` 신규(backend 직접 선택 + 폴백은 단순화 또는 동일 backend만). 기존 `generate_one`/`generate_batch`는 None 전달로 무변경 | 중(시그니처 변경 광범위, 회귀) | - |
| B | LiveSession persona_meta + add/remove | `PersonaMeta{backend, system_prompt, modifier}` + `persona_meta` 필드. `add_persona(assembled, auto_backend)` / `remove_persona(id)`가 personas/intensities/excitations/config.alpha/speaker_labels/store(join·leave)/persona_meta 갱신. `Job` 확장(backend+prompt 실음), 워커가 `generate_on` 호출. 기존 3명 persona_meta로 통일. 단위 테스트(상태 일관·결정성·기존 3명 보존) | 높음(결정성, 워커 채널, 상태 동기화) | A, C |
| C | memory.rs store.leave | `MemoryStore::leave(room, persona)`로 participation 제거(friend-engine on/off 양쪽). events 보존. recall이 leave 후 그 persona 격리 반영 단위 테스트 | 낮음 | - |
| D | web.rs Invite/Remove 프레임 | `ClientFrame::Invite{blood,mbti,zodiac,role?}` / `Remove{id}` + `EngineCmd::Invite/Remove`. `build_state`를 session 기반 동적 participants/models로(현재 시작 시 1회 스냅샷 -> session.persona_meta에서 매번). `types/index.ts` 계약 일관 | 중(participants 동적, 계약) | B |
| E | 프런트 초대 UI | 빈/부분 방 초대 패널: 혈액형/MBTI/별자리 드롭다운 -> `indian_name` 미리보기 -> 추가(최대 3명) + persona 카드에 remove. pnpm build | 중(UI/IME/상태) | D |

Phase A(prompt override + generate_on) + C(leave) -> B(엔진 동적) -> D(web 프레임) -> E(프런트). A·C는 독립이라 병렬 가능.

## 4. 데이터 모델 / 아키텍처 델타

| 구조 | 변경 |
|------|------|
| `ollama.rs` `generate_shared` / `openai.rs` `generate` | 인자에 `system_prompt_override: Option<&str>`. Some이면 그 prompt로 system 메시지 구성, None이면 기존 `system_prompts.get(speaker)`. api_key 로깅 금지 유지 |
| `pool.rs` `Backend::generate` | `system_prompt_override` 인자 추가 후 Ollama/OpenAI에 전달 |
| `pool.rs` `BackendPool::generate_on` | 신규 `pub fn generate_on(&self, backend_name: &str, speaker, history, tick, recall, system_prompt: Option<&str>) -> Option<String>`. `backends.get(backend_name)`에서 직접 생성(세마포어 acquire 포함). 폴백은 명시 backend만(또는 fallbacks 체인 재사용) |
| `live.rs` `PersonaMeta` | 신규 `struct PersonaMeta{ backend: String, system_prompt: String, modifier: PersonaModifier }` |
| `live.rs` `LiveSession` | 필드 `persona_meta: BTreeMap<PersonaId, PersonaMeta>` 추가. `add_persona`/`remove_persona`/`personas()`(이미 있음) + 접근자 |
| `live.rs` `Job` | `(usize, PersonaId, Vec<Event>, u64, Option<String>)` -> backend·system_prompt 추가. 워커가 `pool.generate_on(backend, speaker, history, tick, recall, Some(prompt))` 호출 |
| `live.rs` tick 디스패치 | `chosen`의 backend/prompt를 persona_meta에서 조회해 job에 실음 |
| `memory.rs` `MemoryStore` | `pub fn leave(&mut self, room, persona)`. friend-engine off(Vec/BTreeMap) + on(SQLite) 양쪽 |
| `web.rs` `ClientFrame` | `Invite{blood,mbti,zodiac, role: Option<String>}`, `Remove{id}` variant 추가 |
| `web.rs` `EngineCmd` | `Invite(...)`, `Remove(String)` 추가. handler에서 `assemble`+`indian_name`+자동 backend -> `session.add_persona` |
| `web.rs` `build_state` | participants/models를 `session.personas()` + `session.persona_meta`(backend->model 매핑)에서 매 프레임 산출(시작 스냅샷 제거) |
| `web/src/types/index.ts` | ClientFrame Invite/Remove + participants model 동적 |
| `web/` 컴포넌트 | 초대 패널(드롭다운 + 이름 미리보기 + 추가/제거) |

## 5. 검증

- 골든 5종(`/tmp/salon_golden/`) 바이트 동일(`cargo build` 후 명시적 순차 실행).
- `cargo test`(default 226) / `--features friend-engine`(235) / `--features friend-engine-semantic`(263) green + 신규 add/remove 단위 테스트.
- `cargo build --features web` + 프런트 `pnpm build`(이 셸에서 pnpm 안 되면 `web/node_modules/.bin/{tsc,vite}` 직접).
- 라이브: `--web`으로 초대 -> 새 persona가 자동 이름·자동 backend로 발화에 합류, remove 시 즉시 빠짐. 실 LLM 페이싱은 qwen thinking ~70s 고려.

## 6. 위험과 대응

| 위험 | 대응 |
|------|------|
| system_prompt override가 기존 LLM 경로 회귀 | override는 `Option`, 기존 호출처 전부 `None` 전달. None 경로는 바이트 동일. 단위 테스트로 None=기존, Some=주입 확인 |
| add/remove로 상태 불일치(intensities에 유령 키, alpha 잔여 쌍) | INV-3 일관성 단위 테스트: add->remove->add 시퀀스 후 키셋 4종(personas/intensities/persona_meta/store) 일치 + excitations/alpha 잔여 0 assert |
| 결정성 깨짐(persona 수 변동이 rng 소비 패턴 변경) | 동적 초대는 라이브 전용(LLM 비결정)이라 골든 무관. 단위 테스트는 LLM-off Fake로 상태 전이만 결정적 검증. rrf rng 소비는 후보 수에 의존하나 골든 경로(driver, 고정 3명)는 불변 |
| pool Arc 무가변 위반 유혹(Arc::get_mut 등) | 금지. generate_on은 `&self`만. backend는 컨테이너에서 선택만, 등록 변경 없음(INV-2) |
| web participants 시작 스냅샷이 동적 미반영 | build_state를 session 기반으로 전환(델타 표). 초대/제거 즉시 state 프레임 재전송 |
| qwen 늘면 페이싱 저하(thinking ~70s) | cloud 1/qwen 2 자동배분으로 cloud 우선 채움. 체감 시 tick 주기/ gemma 비중 재조정(별도) |
| 빈 방(0명) 엣지 | add/remove가 0~N 견디게. tick은 0명이면 Silent. web은 초대 전 안내 |

## 7. 이후 단계 (이 트랙의 후속 phase, 별도 task 분해)

1. **영속(서버 SQLite)**: 방 메시지 + 참가자(persona_meta) 저장/복원. 나가기/탭닫힘 시 저장, 재진입 시 이어가기. friend-engine memory.db와 별개의 "방 상태" 저장(또는 통합 검토).
2. **멀티룸**: 방 선택/생성/전환. WS 프로토콜에 room id. LiveSession 다중 인스턴스 or 라우팅.
3. **본인 프로필 + 프리셋**: 사람 프로필(혈/MBTI/별 + 자동 이름) + 프리셋 8개 + 저장된 프리셋. persona_kit 재사용.
4. **비주얼(픽셀아트)**: persona_kit VisualHint 렌더. 외부 이미지 에셋 도착 후(보류).

## 8. 산출물

- 이 문서. 착수 시 §3을 task로 분해(`salon-web-ux-task-NN.md`).
- 한 줄: pool은 고정 컨테이너로 두고 LiveSession이 persona_meta로 라우팅/프롬프트 전권을 쥐어, 런타임에 persona를 자동 이름·자동 모델로 초대/퇴장시킨다. pool은 Arc 그대로, 골든은 안 깨진다.

## 9. 2단계 상세: 영속(단일방) — in_progress

> 1단계(동적 초대) 완료 후 착수(2026-06-03 사용자 "영속 먼저(단일방)"). 멀티룸 전이라 방은 "salon" 하나. 방 대화/참가자/화제를 SQLite에 저장하고 재접속·프로세스 재시작 시 이어간다. 스키마·생명주기를 작게 확립해 이후 멀티룸에서 재사용.

### 9.1 핵심 결정
- **DB = 별 파일 `rooms.db`**(friend engine `memory.db`와 독립). 경로 `$SALON_ROOMS_DB` -> `$HOME/.local/share/tunaSalon/rooms.db`(memory.rs `default_db_path` 패턴 차용). `rusqlite`를 **web feature에도 추가**(`web = [.., "dep:rusqlite"]`) -> web만 켜도 영속(friend-engine 불요). friend engine 회상 events와 용도 분리(회상 vs 방 복원).
- **저장 대상**: participants(`personas`{id,name,base_rate} + `persona_meta`{backend,system_prompt,modifier}), messages(`state.history`의 완성 발화: ts/speaker/mark/content), topics, tick_count. **강도(intensities/excitations)는 미저장** -> 복원 시 base_rate에서 재차오름(자연스러운 재개). pending(content=None) placeholder는 저장 제외. human("나")은 participants에 미포함(Hawkes 외부, 고정).
- **저장 시점**: `run_engine` 주기(`SAVE_PERIOD`, dirty 시) + `Shutdown` 시. WS 마지막 끊김은 broadcast라 감지 곤란 -> 주기 저장이 "탭 닫힘"을 커버. 명시 "나가기" 버튼 저장은 후속(task H, 선택).
- **복원**: `serve` 시작 시 `RoomStore.load("salon")`이 Some이면 그 snapshot으로 LiveSession 구성(빈 personas로 `with_store` -> `add_persona`들 + `restore_history` + `set_topics` + tick_count), None이면 기존 기본 3명(`build_demo_persona_meta`).

### 9.2 Invariants
| ID | 내용 |
|----|------|
| P-INV-1 | 골든/엔진 테스트 무영향: web feature 전용, driver/headless 불침투. `rooms.db`는 라이브 web 경로만 |
| P-INV-2 | friend engine과 독립: `rooms.db` 별 파일, friend-engine feature 불요. memory.db 스키마 불변 |
| P-INV-3 | 비밀 비노출: participants에 system_prompt 저장되나 api_key는 어디에도 없음(INV-5 계승) |
| P-INV-4 | 복원 일관성: 같은 snapshot -> 같은 참가자·로그·화제. 강도만 휘발(재차오름). 빈 DB는 기본 3명 |

### 9.3 Subtasks
| task | 제목 | 핵심 | 위험 | depends_on |
|---|---|---|---|---|
| F | RoomStore SQLite | `web = [.., "dep:rusqlite"]`. 신규 `src/roomstore.rs`(web feature): `RoomStore{conn}`, `open(path)`/`default_rooms_db_path()`(WAL, IF NOT EXISTS), `save(room_id, personas, persona_meta, history, topics, tick_count)`, `load(room_id) -> Option<RoomSnapshot>`. 단위 테스트(save->load 라운드트립, 빈 방 None, content=None 제외) | 중(스키마·직렬화) | - |
| G | LiveSession 복원 + web 배선 | live.rs `restore_history(Vec<Event>, tick_count)` 주입 메서드. web.rs `serve` 시작 load->복원(or 기본3명), `run_engine` 주기(SAVE_PERIOD)+Shutdown save + dirty 플래그 | 중~높(복원 경로·생명주기) | F |
| H(선택) | 명시 나가기 저장 | 프런트 "나가기" 버튼 -> `ClientFrame::Leave` -> 즉시 save. 주기 저장으로 충분하면 생략 | 낮음 | G |

### 9.4 데이터 모델
- `room_participants(room_id TEXT, ord INT, persona_id TEXT, name TEXT, base_rate REAL, backend TEXT, system_prompt TEXT, reactivity REAL, provocativeness REAL, PRIMARY KEY(room_id, persona_id))`
- `room_messages(room_id TEXT, seq INT, ts REAL, speaker TEXT, mark REAL, content TEXT, PRIMARY KEY(room_id, seq))` — 완성 발화만(content NOT NULL)
- `room_meta(room_id TEXT PRIMARY KEY, topics_json TEXT, tick_count INTEGER, updated_at INTEGER)`
- `save` = 트랜잭션으로 `DELETE WHERE room_id` 후 전량 INSERT(단일방이라 비용 작음, 단순·정합).

### 9.5 검증
- `cargo build --features web` + 신규 roomstore 단위 테스트(save/load 라운드트립). default/골든 무영향(web cfg).
- 라이브: `--web` 띄워 초대/대화 -> 종료(또는 주기 저장 대기) -> 재시작 시 참가자·로그·화제 복원 확인.
