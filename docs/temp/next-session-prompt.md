---
title: 다음 세션 첫 대화 복붙 프롬프트 (refactoring review → task-50 = v0.10 마감)
type: reference
status: active
updated_at: 2026-06-03
---

# 다음 세션 킥오프 프롬프트

아래 블록을 새 세션 첫 메시지로 복붙하세요. (이 세션에서 v0.10 Stage 2a/b/c 완료, task-50 마감만 남음. task-50 전 리팩토링 리뷰 먼저.)

---

```
tunaSalon 이어서 작업한다. 먼저 CLAUDE.md(핸드오프)와 docs/plans/salon-engine-v10.md, 그리고 최근 커밋(git log --oneline -15)을 읽어 현재 상태를 파악해.

현재: v0.1~v0.9 완료. v0.10(friend engine Stage 2=의미검색)은 2a(임베더 embed.rs)·2b(usearch ANN ann.rs)·2c(hybrid RRF 회상 memory.rs)까지 완료·커밋. 전부 friend-engine-semantic feature 뒤, MockEmbedder로 결정적 테스트. ORT BGE-M3 in-process 실측 viable(로드 3.8s/embed 29ms/2.3GB, download-binaries, 모델은 ~/.cache/tunaSalon/models/bge-m3/에 받아둠). default 225/friend-engine 234/semantic 260 tests, 골든 무손상.

순서:
1) **먼저 리팩토링 리뷰**: docs/plans/refactoring-review-v9-snapshot.md 를 작성해라. 긴 세션 동안 memory.rs에 cfg 다중 impl(Vec/SQLite/semantic)·recall 중복(non-semantic vs semantic)·embed.rs/ann.rs가 쌓였다. 코드를 읽고 (a) 중복·복잡도·cfg 스프롤, (b) 정리하면 좋을 부분, (c) 위험 낮은 리팩토링 제안을 정리해라. 골든/feature off-on 불변식은 절대 보존(헤드리스 golden 5종 + friend-engine·semantic 테스트). 리뷰 후 안전한 정리만 적용(큰 재설계는 제안만).

2) **그다음 task-50 = v0.10 마감**(docs/plans/salon-engine-v10.md §3 task 50):
   - 실 OrtEmbedder를 라이브 live_store(memory.rs)에 배선: 모델 있으면 OrtEmbedder, 없으면 MockEmbedder 폴백(loud 경고). 테스트/:memory:는 Mock 유지(모델 로드 0). 주의: 임베더는 DB당 일관해야 한다(Mock/Ort 혼용 시 벡터공간 불일치 → ANN 재구축 필요; 최소한 경고/문서화).
   - 실모델 의미 #[ignore] 테스트: "어휘는 다른데 의미는 같은" 케이스(예: 저장 "강아지 산책시켰어" / 쿼리 "반려동물 데리고 나갔어")가 OrtEmbedder hybrid로 회상되는지(BM25만으론 못 잡음). 모델 ~/.cache/tunaSalon/models/bge-m3/ 사용.
   - smoke_v10(v0.10 계약 게이트) + README.md/README.ko.md/CLAUDE.md/index v0.9→v0.10 bump.
   - 골든 5종 + 기본/friend-engine/semantic 테스트 전부 green 유지.

구현은 Sonnet 서브에이전트(Agent tool, model sonnet)에 위임, Claude(Opus)가 스펙·리뷰·커밋. codex 비사용. seCall(~/privateProject/seCall) 검색코어가 lift 원본. 최종 답변은 한국어, em-dash 금지.
```

---

## 참고(핸드오프 보강)

- **리팩토링 후보**(리뷰에서 판단): memory.rs의 cfg 분기가 깊다(friend-engine 안에 friend-engine-semantic 중첩, recall 2벌, sqlite_impl 비대). embed/ann은 feature-gated 신규라 비교적 깨끗.
- **task-50 위험**: live_store에서 OrtEmbedder 로드가 --chat 시작 3.8s 지연(1회). lazy 로드로 미루면 첫 회상 때 지연. 둘 중 택. 테스트가 실모델 로드 안 하게 격리 필수(open()은 Mock, live_store만 Ort).
- **web 트랙**: Kimi 초안 `web/`(React/Vite, 엔진상태 패널 강점, 중앙 3D 큐브·채팅영역 미완). 데이터 계약은 docs/temp/salon-web-ui-kimi-prompt.md.
- 골든 베이스라인 5종은 /tmp/salon_golden/(레포 밖). 비교는 cargo build 후 명시적 순차(zsh `set --` 워드스플릿 안 됨 주의).
