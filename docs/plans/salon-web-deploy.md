---
type: handoff
status: active
updated_at: 2026-06-27
---

# tunaSalon 웹 배포 핸드오프 (Windows → MacBook 이어가기)

> 목적: 지금까지 로컬(Windows, 8080 백엔드 + 6173 Vite)에서 굴리던 웹 제품을 **MacBook에서
> 이어 배포**한다. 이 문서는 Mac에서 clone/pull 후 바로 빌드·실행·배포를 이어가기 위한 모든 것.
> 레포에 커밋되므로 Mac은 `git pull`로 이 문서를 받는다.

---

## 0. 한 줄 요약

배포는 **단일 Rust 바이너리**가 정답이다 — 백엔드(axum)가 `/api`·`/ws`(WebSocket)와 **프런트
정적 빌드(`web/dist`)를 한 오리진에서 같이 서빙**한다. 즉 프런트를 빌드해 `web/dist`를 만들고,
백엔드를 `--web --host 0.0.0.0`으로 띄우면 그게 곧 배포본이다. **유일한 까다로운 의존성은
"클라우드 LLM 모델이 로컬 `ollama` 데몬(클라우드 로그인됨)을 거친다"** 는 점 — 배포 호스트에
`ollama`가 설치·로그인돼 있어야 한다(§3, §7).

---

## 1. 현재 상태 (이번 세션까지, 전부 main에 push됨)

최신 커밋 `a8fc6a5` 기준. 공개 레포 `github.com/hang-in/tunaSalon`.

웹 제품(`--web`)에 이번 세션에 들어간 것:
- **단계형 토론**: 오프닝→입장→공방→클로징→종료(발화수+수렴), 종료 시 dispatch 중단·사용자 발화로 재진입.
- **절차적 SVG 캐릭터 아바타**(`web/src/lib/personaAvatar.tsx`): 혈액형=얼굴색, 별자리=머리색+심볼, 역할=머리모양, MBTI=얼굴형/눈/눈썹, λ=포즈(졸음→손번쩍→발화).
- **페르소나 4축 영속**(rooms.db) + **사람(나) 캐릭터**(나 카드 클릭→4축, 닉네임 표시).
- **사이드바 게이지 색 구분**(혈액형 팔레트/id 해시 통일).
- **토론 모델 선택**: 7종 클라우드 모델(`model.rs CLOUD_MODELS`)에서 3개 골라 페르소나에 1:1 배정(설정 기어). backend 이름=모델 태그 → `PersonaMeta.backend`로 영속.
- **역할 잠정 폐기**: 개성은 혈액형+별자리+MBTI만(각 2문장으로 강화). 역할은 아바타 머리 코스메틱.
- **닉네임 한글 툴팁**(디버깅): MBTI/혈액형/별자리 성격 설명.
- **종료 리포트**: 메타 분석가가 두괄식 마크다운 리포트 생성 → 전용 카드 렌더 → rooms.db 영속 → 재접속 재표시 → **로비 카드에 "결론 남" 배지 + 결론 요약**.

> 친구 vLLM 서버(`yongseek.iptime.org:8008`, qwen3.6-35b-fast)는 **다운 상태** → 현재 **cloud-only**로 동작(정상 폴백). 배포도 cloud-only 전제.

---

## 2. 배포 아키텍처 (단일 바이너리)

```
[브라우저] ──HTTP/WS──> [salon --web :8080]
                          ├─ GET /            → web/dist/index.html (SPA)
                          ├─ GET /assets/*    → web/dist (정적)
                          ├─ GET /api/suggested-topics
                          ├─ WS  /ws?room_id=&topic=&personas=&models=
                          └─ 엔진 스레드(방마다) ──reqwest──> localhost:11434 (ollama 데몬)
                                                                      └─> Ollama Cloud (:cloud 모델)
[rooms.db] 방 영속(참가자·메시지·4축·사람·리포트)   [memory.db] friend engine 회상
```

- **개발**: 백엔드 `:8080`(127.0.0.1) + Vite `:6173`(0.0.0.0)이 `/api`·`/ws`를 8080으로 프록시. (HMR 편의용)
- **배포(prod)**: Vite 불필요. `web/dist`를 백엔드가 직접 서빙 → **한 포트, 한 프로세스**.
- 멀티룸: WS 접속의 `room_id`마다 방 런타임을 spawn. 방 상태는 rooms.db에 영속.

---

## 3. 하드 의존성 (배포 전 반드시)

### 3-1. ⚠️ 클라우드 LLM = 로컬 `ollama` 데몬 경유 (가장 중요)
- 페르소나 발화·종료 리포트는 백엔드가 `http://localhost:11434`(로컬 ollama 데몬)에 `:cloud` 모델로 요청 → 데몬이 Ollama Cloud로 프록시.
- 따라서 **배포 호스트에 `ollama`가 설치돼 있고 Ollama Cloud 계정으로 로그인**돼 있어야 한다(`ollama signin`). 로그인 안 되면 모든 발화가 실패→침묵.
- 선택한 7개 모델이 그 계정에서 실제 접근 가능해야 함(`ollama run <tag>` 로 사전 확인 권장). 안 되는 모델은 자동으로 `gemma4:31b-cloud`로 폴백(코드에 설정됨).
- **로컬 ollama로 `:cloud`가 아닌 모델을 돌리는 건 금지**(맥북 랙) — 가드가 거부. `:cloud` 태그만.

### 3-2. `.env` (gitignore → Mac에서 재생성 필수)
- `.env`는 커밋 안 됨. **Mac에서 새로 만들어야 함**: `cp .env.example .env` 후 키 채우기.
- `OLLAMA_CLOUD_API_KEY` = 로비 추천 주제 웹서치(`/api/suggested-topics`)에 사용. 비면 추천 주제는 정적 폴백(앱은 정상 동작). 형식 `4cf…`(Windows의 기존 .env에서 복사해 오면 됨).
- 참고: 발화용 cloud 모델은 데몬 로그인으로 동작(키 불요), 웹서치만 이 키 필요 — 두 경로가 다름.

### 3-3. 영속 파일 (백업/이전 대상)
- `rooms.db`: 방 영속(참가자·메시지·4축·사람 캐릭터·리포트). 경로: `$SALON_ROOMS_DB` → `$HOME/.local/share/tunaSalon/rooms.db`.
- `memory.db`: friend engine 회상(web feature가 friend-engine 포함). 경로: `$SALON_MEMORY_DB` → `$HOME/.local/share/tunaSalon/memory.db`.
- Windows의 방 데이터를 Mac으로 옮기려면 위 두 파일 복사(선택). 안 옮기면 새로 시작(무방).

### 3-4. Redis (선택 — 없어도 됨)
- `redis-bus` 기능은 멀티 인스턴스 조율용. `SALON_REDIS_URL`이 없으면 redis 없이 단일 인스턴스로 동작. **단일 호스트 배포면 redis 불요.**

---

## 4. 환경 변수

| 변수 | 용도 | 배포 시 |
|---|---|---|
| `OLLAMA_CLOUD_API_KEY` | 로비 추천 주제 웹서치 | `.env`에 채움(없으면 정적 폴백) |
| `SALON_ROOMS_DB` | rooms.db 경로 override | 선택(기본 `$HOME/.local/share/...`) |
| `SALON_MEMORY_DB` | memory.db 경로 override | 선택 |
| `SALON_REDIS_URL` | 멀티 인스턴스 | 단일 호스트면 미설정 |
| `SALON_CLOUD_ONLY` | friend 서버 무시·cloud만 | friend 다운이므로 켜둬도 됨(현재 자동 폴백됨) |
| `SALON_DEBATE_THINKING` | 데모 cloud thinking on/off(기본 on) | 그대로 |
| `SALON_LANG` | 응답 언어(기본 한국어) | 그대로 |

---

## 5. Mac 사전 준비

```bash
# 1) 툴체인
#   - Rust: https://rustup.rs  (rustup, cargo)
#   - Node + pnpm:  brew install node && npm i -g pnpm   (또는 npm 사용)
#   - Ollama:       brew install ollama   (또는 ollama.com 앱)

# 2) Ollama 클라우드 로그인 (필수)
ollama serve &                 # 데몬 (앱 설치 시 자동 실행되기도 함)
ollama signin                  # Ollama Cloud 계정 로그인
ollama run gemma4:31b-cloud "한국어로 한 줄 인사"   # 클라우드 접근 확인

# 3) 레포 + .env
git clone https://github.com/hang-in/tunaSalon.git
cd tunaSalon
cp .env.example .env
#   .env 의 OLLAMA_CLOUD_API_KEY 를 Windows .env 값으로 채운다 (웹서치용)
```

> rusqlite는 `bundled` 빌드라 **시스템 sqlite 설치 불요**. web 빌드는 ort/onnx/coreml **불필요**(그건 friend-engine-semantic 전용). 즉 Mac web 빌드는 순수 cargo로 끝.

---

## 6. 빌드 & 실행

### 6-1. 개발(HMR) — Windows에서 하던 방식의 Mac판
```bash
# 백엔드 (별 터미널)
cargo run --features "web redis-bus" -- --web --host 127.0.0.1 --port 8080 \
  --room-id debate-ai-open-source --topic "AI 규제와 오픈소스"
# 프런트 (별 터미널)
pnpm -C web dev --host 0.0.0.0 --port 6173      # http://localhost:6173
```

### 6-2. 배포(prod) — 단일 바이너리, Vite 없이
```bash
# 1) 프런트 정적 빌드 → web/dist
pnpm -C web install
pnpm -C web build

# 2) 백엔드 릴리즈 빌드
cargo build --release --features "web redis-bus"

# 3) 실행 (레포 루트에서! 백엔드가 ./web/dist 를 서빙한다)
./target/release/salon --web --host 0.0.0.0 --port 8080 \
  --room-id debate-ai-open-source --topic "AI 규제와 오픈소스"
#   → http://<host>:8080 에서 SPA + API + WS 모두 제공
```
- `--host 0.0.0.0` 이라야 LAN/외부 접속 가능(127.0.0.1은 로컬만).
- **반드시 레포 루트에서 실행** — 정적 서빙이 상대경로 `web/dist`를 찾는다(로그에 "정적 서빙 …/web/dist" 확인).
- macOS 백그라운드 실행: `nohup ... > run.log 2>&1 &` 또는 `launchd`/`pm2`/`tmux`. (이 레포 CLAUDE.md는 에이전트의 `&` 사용을 금하지만, 사람이 직접 배포 실행하는 건 무관.)

---

## 7. 배포 옵션 (결정 필요)

핵심 제약: **호스트에 ollama(클라우드 로그인)가 떠 있어야 한다.** 이게 옵션을 가른다.

### A. 맥북을 그대로 호스트로 + 터널 (가장 빠른 다음 단계 · 추천)
- Mac에서 §6-2로 띄우고, 외부 공개는 **Cloudflare Tunnel**(`cloudflared tunnel --url http://localhost:8080`) 또는 **Tailscale**/**ngrok**.
- 장점: ollama가 Mac 네이티브로 잘 돎, 설정 최소. 데모·지인 공유에 충분.
- 단점: 맥북 켜져 있어야 함(상시 서비스엔 부적합).

### B. 상시 서버(VPS/홈서버)에 ollama + 바이너리
- 서버에 `ollama` 설치 + `ollama signin`(사용자 Ollama Cloud 계정) + 바이너리 실행.
- 장점: 상시 가동. 단점: 서버에 GPU 불요(클라우드 프록시라서)나 ollama 로그인 세션 관리 필요, GPU시간 정액 구독 비용은 계정에 종속.
- friend vLLM을 살리면(지인서버 복구) 부하 분산 가능.

### C. 프런트(정적 호스트) + 백엔드 분리
- `web/dist`를 Vercel/Netlify로, 백엔드만 ollama 있는 곳에. **단일 오리진 장점을 잃고 CORS/프록시 설정이 늘어난다** → 지금은 비추천(단일 바이너리가 더 단순).

**추천 경로**: 우선 **A(맥북 + Cloudflare Tunnel)**로 외부 접속을 띄워 데모를 굴리며, 상시화가 필요해지면 B로. C는 로그인/멀티유저까지 간 뒤 고려.

### 미결정(다음 세션에서 사용자와)
1. 어디에 배포? (맥북+터널 / VPS / 기타)
2. 도메인·HTTPS 필요? (터널이면 자동 https 서브도메인 제공)
3. 로그인/멀티유저? (현재 로비는 **브라우저 localStorage** 기반 — 서버 공유 로비/계정은 별도 트랙)
4. 상시 가동 시 프로세스 관리(launchd/pm2/systemd) + ollama 데몬 상시화.

---

## 8. Windows → Mac 함정

- **`.env`는 안 따라온다**(gitignore) → Mac에서 재생성(§3-2).
- 바이너리 이름: Windows `salon.exe` → Mac `salon`(확장자 없음).
- 셸: PowerShell 아님 → zsh/bash. `$env:VAR` → `export VAR=`, `Start-Process` → `nohup …&`.
- 경로: `D:\privateProject\tunaSalon` → `~/…/tunaSalon`. `$HOME` 기반 영속 경로는 Mac에서 자동 정상.
- 줄바꿈: 레포에 CRLF/LF 혼재 경고가 있으나 빌드엔 무해(`.gitattributes` 없으면 git이 알아서).
- **함정(기존)**: `--web`은 반드시 `--features "web redis-bus"`로 빌드. default 빌드로 만든 바이너리는 web 없이 즉시 종료.
- 빌드 중 실행 중인 바이너리가 있으면 링크 충돌(특히 재빌드) → 빌드 전 기존 프로세스 정지.

---

## 9. 배포 검증 체크리스트

1. `ollama signin` 됨 + `ollama run gemma4:31b-cloud "..."` 한국어 응답 OK.
2. `cargo build --release --features "web redis-bus"` 성공.
3. `pnpm -C web build` → `web/dist/index.html` 생성.
4. 레포 루트에서 `./target/release/salon --web --host 0.0.0.0 --port 8080 …` → 로그에 "web 서버: http://0.0.0.0:8080" + "정적 서빙 …/web/dist".
5. 브라우저 `http://localhost:8080` → 로비 뜸 → 새 방 → 페르소나 3명 한국어 발화 → 공방 후 종료 → **리포트 카드** 뜸 → 로비에 "결론 남" 배지.
6. (외부) Cloudflare Tunnel URL로 다른 기기에서 접속 확인.
7. 재시작 후 방/리포트/4축 유지(rooms.db 영속) 확인.

---

## 10. Mac 세션 첫 프롬프트(복붙용)

> tunaSalon 웹배포 이어서(맥북). `docs/plans/salon-web-deploy.md` 읽었어.
> 먼저 ① ollama 클라우드 로그인 확인(`ollama run gemma4:31b-cloud`) ② `.env` 생성(OLLAMA_CLOUD_API_KEY) ③
> `pnpm -C web build` + `cargo build --release --features "web redis-bus"` ④ 레포 루트에서 prod 실행 →
> localhost:8080 동작 확인. 그 다음 배포 옵션(맥북+Cloudflare Tunnel vs VPS)을 정하자.
> 미결정: 배포 위치 / HTTPS·도메인 / 로그인·서버 로비 / 상시 가동 프로세스 관리.

> 메모리: [[debate-producer-track]] [[human-character-profile]] [[avoid-god-files]]
