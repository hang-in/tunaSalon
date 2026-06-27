---
type: handoff
status: active
updated_at: 2026-06-27
---

# tunaSalon 상시 배포 핸드오프 (홈랩 Caddy + n100)

> 목적: tunaSalon 웹 제품을 **`salon.d9ng.co.kr`로 상시 공개**(나만 로그인) 배포한다.
> 호스트는 홈랩 **n100(192.168.1.121)**, 진입은 기존 **홈랩 Caddy**(external/internal 2계층),
> 로그인은 **Caddy basic_auth**. 배포 인프라 SSOT = `~/privateProject/homelab-proxy`.
>
> ⚠️ 이전 버전(Cloudflare Tunnel + Access 전제)은 **폐기**. 홈랩은 Cloudflare 터널을 쓰지 않고
> WireGuard 메시 + Caddy로 굴린다. 이 문서가 그 사실을 반영한 현행이다.

---

## 0. 한 줄 요약

tunaSalon은 **destiny-tribe와 동일한 "비-`.pages` 풀스택 → n100" 패턴**으로 배포한다.
n100에서 **단일 Rust 바이너리**(`salon --web`, web/dist 동봉)를 systemd로 상시 실행 → 홈랩 Caddy가
`salon.d9ng.co.kr`을 `192.168.1.121:5181`로 reverse_proxy → external 블록의 **basic_auth**로 나만 통과.
클라우드 LLM은 **n100 로컬 `ollama` 데몬(:cloud)** 경유(현 코드 무수정 경로).

> `.pages.d9ng.co.kr`은 **Gitea(git.d9ng.co.kr) 레포 임시배포 전용**. tunaSalon은 GitHub 개인
> 프로젝트라 그 파이프라인을 쓰지 않고 **직접 서브도메인 `salon.d9ng.co.kr`**을 쓴다(사용자 확정).

---

## 1. 확정 결정 (이번 세션, 2026-06-27)

| 항목 | 결정 | 근거 |
|---|---|---|
| 서브도메인 | **`salon.d9ng.co.kr`** | 제품명·짧음. `.pages`는 Gitea 전용. d9ng.co.kr 와일드카드 없음→개별 CNAME 1개 추가 |
| 호스트 | **n100 = `192.168.1.121`**(16G, docker, **fish shell**) | destiny와 동일. ollama 동거 가능(:cloud=프록시, 로컬 GPU 불요) |
| 실행 형태 | **bare 바이너리 + systemd**(docker 아님) | web/dist 컴파일타임 경로 함정 회피 + .env cwd 로드 자연 + 의존 가벼움(rusqlite) |
| 포트 | **5181**(destiny=5180 다음 빈 포트) | Caddy → `192.168.1.121:5181` |
| 진입(공개) | 홈랩 **Caddy** external(oci-ampere, geo_kr_only) + internal(caddy-internal) | WireGuard 메시로 external이 LAN IP 직접 도달(기존 인프라) |
| 로그인 | **Caddy `basic_auth`**(external 블록, bcrypt) | 홈랩에 SSO/forward-auth 레이어 없음(grep 0건). "나만 쓰는"엔 앱 코드 0 |
| 배포 트리거 | **수동/스크립트**(GitHub 레포라 Gitea CI 자동배포 미사용) | 개인 프로젝트엔 충분. push-to-deploy 원하면 Gitea 미러는 이후 트랙 |
| 클라우드 LLM | **n100 로컬 ollama 데몬(:cloud)**, 코드 무수정 | 빠른 첫 배포. 데몬 제거(=ollama.com/v1 직결)는 이후 트랙(§7) |

---

## 2. 배포 아키텍처

```
[브라우저] ──https──> [external Caddy @ oci-ampere 158.180.66.24]   (geo_kr_only + basic_auth)
                          │  reverse_proxy (WireGuard 메시)
                          ▼
                       [n100 192.168.1.121:5181]  ← salon --web --host 0.0.0.0 (systemd)
                          ├─ GET /            → web/dist/index.html (SPA)
                          ├─ GET /api/*       → suggested-topics 등
                          ├─ WS  /ws          → 방 런타임(멀티룸)
                          └─ reqwest ──> n100 localhost:11434 (ollama 데몬) ──> Ollama Cloud (:cloud)
[LAN 접속] ──> [internal Caddy @ prox-docker caddy-internal] ──> 192.168.1.121:5181  (redirect_lan_http)
[rooms.db]/[memory.db]  n100 의 repo 경로 또는 $HOME/.local/share/tunaSalon/
```

- **Caddy 라우트는 push-to-deploy**: `homelab-proxy` main에 commit+push 하면 각 호스트의 `caddy-sync.sh`
  cron(매분)이 `git pull`+rsync+`caddy reload` 자동 수행. 수동 ssh reload는 폴백.
- destiny 템플릿(`homelab-proxy` 커밋 `d7ccf41`, operations.md 2026-06-23)이 정확히 같은 모양.

---

## 3. 사전 준비 — n100 (192.168.1.121)

> n100은 **fish shell**. 복잡한 bash one-liner는 깨질 수 있으니 단순 명령 또는 `bash -c '...'`.

1. **툴체인 확인/설치**: `rustup`(cargo), `ollama`, node+`pnpm`. (n100에 이미 있는지 먼저 확인 — immich/destiny가 도는 박스라 일부 존재 가능.)
2. **ollama 클라우드 로그인 (필수·하드 의존)**:
   ```bash
   ollama serve        # systemd 유닛으로 이미 떠 있을 수 있음
   ollama signin       # Ollama Cloud 계정
   ollama run gemma4:31b-cloud "한국어로 한 줄"   # 클라우드 접근 확인
   ```
   선택한 7개 cloud 모델(`src/model.rs CLOUD_MODELS`)이 그 계정에서 접근 가능해야. 안 되는 건 코드가 `gemma4:31b-cloud`로 폴백.
3. **레포 + .env**:
   ```bash
   git clone https://github.com/hang-in/tunaSalon.git
   cd tunaSalon
   cp .env.example .env      # OLLAMA_CLOUD_API_KEY 채움(로비 추천주제 웹서치용. 발화는 데몬 로그인으로 동작)
   ```
   `OLLAMA_CLOUD_API_KEY`는 tunaLlama `~/privateProject/tunaLlama/.env`의 작동키 재사용 가능(operations.md 패턴, prefix `4cf…`).

---

## 4. 빌드 & systemd 상시화 (n100)

```bash
cd ~/tunaSalon   # (실제 배포 경로로)

# 1) 프런트 정적 빌드 → web/dist
pnpm -C web install
pnpm -C web build

# 2) 백엔드 릴리즈 빌드 (N100는 Mac보다 느림, 일회성)
cargo build --release --features "web redis-bus"
```

systemd 유닛 `/etc/systemd/system/tunasalon.service` (WorkingDirectory가 .env + web/dist의 기준):

```ini
[Unit]
Description=tunaSalon web (salon.d9ng.co.kr)
After=network-online.target ollama.service
Wants=network-online.target

[Service]
Type=simple
User=d9ng
WorkingDirectory=/home/d9ng/tunaSalon
ExecStart=/home/d9ng/tunaSalon/target/release/salon --web --host 0.0.0.0 --port 5181 \
  --room-id salon-main --topic "오늘의 잡담"
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now tunasalon.service
sudo systemctl status tunasalon.service     # active + 로그에 "web 서버: http://0.0.0.0:5181"
curl -s localhost:5181/ | grep -o '<title>[^<]*</title>'   # SPA 확인
```

- `--host 0.0.0.0` 필수: Caddy(external, WireGuard 너머)가 `192.168.1.121:5181`로 닿아야 함.
- LAN에서 `192.168.1.121:5181` 직접 접속은 basic_auth를 우회함(홈 네트워크라 허용 가정). 막으려면 internal 블록에도 basic_auth.

---

## 5. Caddy 라우트 + DNS (homelab-proxy)

### 5-1. external/internal Caddyfile에 salon 블록 추가 (destiny와 동형)

`~/privateProject/homelab-proxy/external/Caddyfile` — destiny 블록 근처에:
```caddy
# salon: tunaSalon 웹 — n100(192.168.1.121:5181). basic_auth(나만).
salon.d9ng.co.kr {
	import site_common
	import geo_kr_only
	basic_auth {
		d9ng <BCRYPT_HASH>
	}
	reverse_proxy 192.168.1.121:5181
}
```

`~/privateProject/homelab-proxy/internal/Caddyfile` — destiny 블록 근처에:
```caddy
http://salon.d9ng.co.kr, https://salon.d9ng.co.kr {
	import site_common
	import redirect_lan_http
	reverse_proxy 192.168.1.121:5181
}
```

bcrypt 해시 생성(Caddy v2): `caddy hash-password --plaintext '<비밀번호>'` → 출력값을 `<BCRYPT_HASH>`에.
(Caddy 버전이 구형이면 directive가 `basicauth`. external caddy 버전 확인 후 맞춤.)

### 5-2. commit + push → cron 자동 반영(~1분)

```bash
cd ~/privateProject/homelab-proxy
git add external/Caddyfile internal/Caddyfile
git commit -m "caddy: salon.d9ng.co.kr → n100(192.168.1.121:5181) + basic_auth"
git push
# caddy-sync.sh(cron 매분)가 prox-docker(internal)/oci-ampere(external)에서 pull+rsync+reload 자동.
# 급하면 수동: ssh prox-docker "docker exec caddy-internal caddy reload --config /etc/caddy/Caddyfile"
```

### 5-3. Cloudflare DNS — `salon` 레코드 1개 (와일드카드 없음)

`d9ng.co.kr` zone에 **`salon` CNAME → `d9ng.co.kr`, proxied=false** 추가(destiny/sol/docad과 동일 패턴). DNS-01 TLS는 internal caddy가 발급.

---

## 6. 검증 체크리스트

1. n100: `ollama run gemma4:31b-cloud "..."` 한국어 응답 OK.
2. n100: `systemctl status tunasalon` active + `curl localhost:5181/` SPA.
3. DNS: `dig +short salon.d9ng.co.kr` 응답.
4. 외부: `curl -sIk https://salon.d9ng.co.kr/` → **401**(basic_auth 게이트 동작) → 인증 후 200 + `via: 1.1 Caddy`.
5. 브라우저 `https://salon.d9ng.co.kr` → basic_auth 프롬프트 → 로비 → 새 방 → 페르소나 3명 한국어 발화 → 토론 종료 → 리포트 카드 → 로비 "결론 남" 배지.
6. 재시작/재부팅 후 systemd 자동 부활 + 방/리포트/4축 영속(rooms.db) 유지.

---

## 7. 함정 & 이후 트랙

### 함정 (검토 중 확인)
- **n100 = fish shell**: bash one-liner/`AV=$(...)` 할당 깨짐(operations.md 반복). systemd/`bash -c` 사용.
- **컴파일타임 정적경로**: `web.rs`의 dist는 `concat!(env!("CARGO_MANIFEST_DIR"),"/web/dist")` 절대경로. **bare 배포(레포에서 빌드·실행)면 무해**. docker로 싸면 런타임 이미지도 같은 경로에 web/dist 둬야 함([[mac-build-env]]).
- **`lobby_topics.rs:76` 하드코딩** `localhost:11434/api/generate`(`--ollama-host` 무시). bare+n100 데몬이면 정상. 컨테이너화하면 깨짐(단 추천주제는 빈 배열로 graceful 폴백, 앱 정상).
- **basic_auth + WS**: 브라우저가 same-origin `/ws` 업그레이드에도 Authorization 헤더 실어보내 통과. OK.
- **메모리 압박**: n100 16G에 immich+destiny(pgvector+e5-large ~2.2G) 동거 → salon(가벼움)+ollama 데몬(프록시, 가벼움) 추가. 모니터.
- **OLLAMA 키 함정**(operations.md): 잘린/무효 키면 401→목업 폴백. `curl https://ollama.com/v1/chat/completions -H "Authorization: Bearer <key>"` 200/401로 검증.

### 이후 트랙 (지금은 미적용)
1. **ollama 데몬 제거 → `https://ollama.com/v1` 직결**: 홈랩 관례(destiny/karakeep)는 로컬 데몬 없이 OpenAI 호환 엔드포인트 직결. salon의 OpenAI 백엔드(`openai.rs`)를 cloud 라우팅에 쓰면 데몬/로그인세션 babysit 불요. 단 코드/설정 변경 + `lobby_topics` native 경로 수정 필요 → **확인 필요**(ollama.com이 native `/api/generate`를 Bearer로 받는지 미검증).
2. **Gitea 미러 + CI push-to-deploy**: destiny식 act_runner(n100 `n100:host` 라벨) 파이프라인. 커스텀 Rust 컨테이너 워크플로 작성 필요(무거움).
3. **멋진 SSO**: 홈랩 전체 forward-auth(Authelia/tinyauth) 도입 시 basic_auth 대체. 별도 인프라 트랙.
4. **친구 vLLM 복구 시**(`yongseek.iptime.org:8008` down) cloud+friend 부하분산. 현재 cloud-only 자동 폴백.

---

## 8. 현재 상태 (참고)

- Windows 세션 작업 전부 main push 완료(`a8fc6a5`): 단계형 토론, 절차적 SVG 아바타, 4축 영속, 사람 캐릭터, 모델 선택(7 cloud), 종료 리포트(마크다운+영속+로비 배지).
- **Mac 로컬 prod 스모크 검증 통과(2026-06-27)**: `pnpm build`+`cargo build --release` OK, `salon --web :8080`에서 `GET /`=200 SPA, `/ws`=101, `/api/suggested-topics`=200(시작 직후 `[]`=백그라운드 생성 전, 버그 아님), cloud-only 자동 폴백. [[mac-build-env]]
- friend vLLM 다운 → cloud-only 전제.

---

## 9. 다음 세션 첫 프롬프트(복붙용)

> tunaSalon 상시 배포 이어서. `docs/plans/salon-web-deploy.md`(홈랩 Caddy/n100 플랜) 읽었어.
> 호스트 n100(192.168.1.121, fish), bare 바이너리+systemd, Caddy(external geo_kr_only+basic_auth / internal),
> salon.d9ng.co.kr:5181, 로컬 ollama 데몬(:cloud).
> 진행: ① n100 SSH 확인 ② 툴체인/ollama signin 확인 ③ clone+.env+build ④ systemd 등록 ⑤ homelab-proxy
> Caddyfile 2개 + basic_auth + push(cron 자동reload) ⑥ Cloudflare salon CNAME ⑦ §6 검증.
> 미결정: ollama 데몬 vs ollama.com/v1 직결(§7-1) / Gitea CI 미러 여부.

> 메모리: [[mac-build-env]] [[ollama-cloud-limits]] [[web-frontend-track]] [[friend-server-vllm]]
> 배포 인프라 SSOT: `~/privateProject/homelab-proxy`(PROJECT_DEPLOY_GUIDE.md, operations.md, caddy-sync.sh)
