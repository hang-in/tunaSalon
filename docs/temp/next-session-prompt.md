---
type: handoff
status: in_progress
updated_at: 2026-06-29
---

# tunaSalon 핸드오프 (다음 세션)

> 표기 규칙: 존댓말, em-dash 금지(일반 대시/콜론), 라이브 도메인 비노출(소스공개·서비스 비공개).
> 구현 위임은 Sonnet 서브에이전트(codex 비사용), Opus가 스펙·리뷰·검증.

## 0. 현재 상태 (origin/main = bf83a1f)

v0.1~v0.10 엔진 위에서 브라우저 토론 앱이 라이브로 돕니다(홈랩 n100, basic_auth). 기본 빌드/headless 골든은 무손상. 테스트: default 306 / friend-engine 319 / semantic 347 / web 356.

## 1. 이번 세션에 한 것 (2026-06-28~29)

**리팩토링 R3 (엔진 코어, golden/recall byte-identical 검증):**
- ① per-tick 결정적 코어 추출(5ce17eb): driver/live 공유 `advance_intensities`/`filter_self_repeat`/`suppress_chosen`. decide_one_tick 전체 통합은 보류(rng 분기 위험).
- ② recall BM25 leg 통합(b397b78): `bm25_leg_ids`/`fetch_events_by_ids`, BM25-only를 2단계로 재구조화.
- ③ LiveSession 디스패치 캡슐화(e8880d9): `GenerationWorker`. 입력(human_focus) 분리는 보류(big-bang).
- god-file 분해: web DTO -> `web/dto.rs`(43aa494), memory `sqlite_impl` -> `memory/sqlite_impl.rs`(d1368c5).

**web 제품 스프린트 (전부 web feature, 골든 무관):**
- 페르소나 4축 말투 디렉티브(0705bb7): `voice_fragment`(MBTI=문장구조/혈액형=대인태도/별자리=전달리듬, 레이어 분리).
- 토론 톤 "친구끼리 가볍게"(d1fc03e): 분석 초점 유지, 레지스터만 완화.
- 채팅방 버전 표시 + 발화자 이름 색상 통일(1033e5c): vite `__BUILD_VERSION__`(git hash), 색상은 사이드바 혈액형 팔레트와 통일.
- 이전 토론 아카이브(dc7e478): `roomstore.list_rooms` + `GET /api/rooms` + ArchivePage. 카드 시작/결론 날짜(47ded83, room_meta.created_at 추가).
- 읽기전용 공유(168073a + 3094ae1 + b17e080): `room_shares`(token) + `POST /api/rooms/{id}/share` + `GET /api/share/{token}`(공개) + ShareView(아바타/4축/성향/MD/RichText). **Caddy 예외 적용·검증됨**(homelab-proxy 9ee57e8: `/share/* /api/share/* /assets/*` basic_auth 제외, geo_kr_only 유지 = 한국만).
- 결론 리포트에 발언자 전원 명시(b95966d): `build_debrief_prompt`에 participants 전달(사람 포함).
- 추천 주제 정체 완화(709fd3a + bf83a1f): 쿼리 풀 회전 + casual 앵글 회전 + gemma temperature 1.0 + 3h 갱신 + **중복 회피**(최근 추천 프롬프트 회피 + 출력 필터 + `recent_topics.json` 영속).
- 문서 리모델링: README 한국어 canonical + 영문 번역(4c42611, 사용자 재구성 49e1ca8, 영문 정렬 e818e92), HISTORY 웹 챕터 추가(ff6500e), **추적 .md 전체 em-dash 제거**(a10ba00).

## 2. 열린 항목 / 확인 필요

- **`~/deploy-salon.sh` 깨짐**(사용자 디버깅 중): 수동 배포는 됨(`cd ~/tunaSalon && git pull && pnpm -C web build && cargo build --release --features "web redis-bus" && sudo systemctl restart tunasalon`). 흔한 원인: 비로그인 셸에서 cargo/pnpm PATH 누락. 견고한 스크립트 재작성 대기.
- **추천 주제 라이브 검증**: 배포 후 `journalctl -u tunasalon | grep 추천`에 "N 분야 생성"이 떠야 정상. "생성 실패"면 웹서치 키/권한 문제 -> 정적 폴백만 뜸(이게 "부먹찍먹 안 바뀜"의 또다른 가능 원인). `recent_topics.json` 생성 확인.
- README.ko.md는 사용자 canonical. 앞으로 README 수정은 한국어 먼저 -> 영문 번역.

## 3. 분리된 새 프로젝트: tunaRound (2026-06-29)

코덱스↔클로드코드 터미널 대화 앱 아이디어는 **별도 레포로 분리됨** = `~/privateProject/tunaRound`. brainstorming 완료 + 설계 spec 승인·커밋(tunaRound 877712f). **더는 tunaSalon 태스크 아님.** 다음은 그 레포에서 writing-plans -> 구현.

- tunaRound 요약: 터미널에서 Codex↔Claude Code가 구조 라운드 토론 -> 수렴하면 결론. tunaFlow(에이전트 구동+Roundtable 포팅) + tunaSalon(Redis 멀티세션 + FlowMeter 수렴) 결합. v1=토론 substrate, v2=협업 코딩.
- 상세: `~/privateProject/tunaRound/docs/design/tunaRound-v1-design.md`, 핸드오프 `~/privateProject/tunaRound/CLAUDE.md`.

## 4. tunaSalon 자체의 다음 작업

tunaSalon은 §2 열린 항목(deploy-salon.sh 견고화, 추천주제 라이브 검증)이 우선. 그 외 보류 후보: god-file 추가 분해(live.rs 2329줄 등, refactoring §3), 입력 모듈 분리(human_focus), 제품 트랙(영속 4축 복원 등 CLAUDE.md §5).
