#![cfg(feature = "web")]
//! web 프런트엔드 sink: axum WebSocket으로 엔진 이벤트를 브라우저에 push + 사람 입력 수신.
//! 엔진은 blocking(전용 스레드), axum은 tokio(async). 둘은 tokio 채널로 브리지.

use crate::live::{LiveSession, PersonaAxes, PersonaMeta};
use crate::persona_kit::{assemble_roleless, Blood, Mbti, Role, Zodiac};
use crate::roomstore::RoomStore;
#[cfg(feature = "redis-bus")]
use crate::session_bus::{RedisBus, RedisBusHandle, SessionBus};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
#[cfg(feature = "redis-bus")]
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc, Mutex};

// ── 프레임 스키마 ──────────────────────────────────────────────

// wire DTO/protocol 타입은 dto 서브모듈로 분리(god-file 분해). 직렬화 형식 무변경.
mod dto;
use dto::{
    ClientFrame, HistoryMessage, Participant, ParticipantAxes, ReportDto, RoomListItemDto,
    RoomReportResponse, ServerFrame,
};

#[allow(dead_code)]
enum EngineCmd {
    Human(String),
    Topic(Vec<String>),
    SetPaused(bool),
    SetPace(u64),
    Invite {
        blood: String,
        mbti: String,
        zodiac: String,
        role: Option<String>,
    },
    Remove(String),
    SetClientCount(usize),
    Reset(Vec<String>),
    SetHumanProfile {
        blood: String,
        mbti: String,
        zodiac: String,
        role: String,
    },
    DeleteAndShutdown,
    Shutdown,
}

const STATE_PERIOD: Duration = Duration::from_millis(700);
const DEFAULT_TICK_MS: u64 = 6000;
const POLL_PERIOD: Duration = Duration::from_millis(80);
const SAVE_PERIOD: Duration = Duration::from_secs(5);
#[cfg(feature = "redis-bus")]
const OWNER_TTL_SECS: u64 = 15;
#[cfg(feature = "redis-bus")]
const OWNER_REFRESH_SECS: u64 = 5;

#[derive(Debug, Clone, Default)]
pub struct WebStartup {
    topics: Vec<String>,
    /// 새 방 시딩용 초기 참가자 스펙(수동 구성). 비어 있으면 랜덤 3명으로 시딩한다.
    /// 복원된 방(rooms.db 스냅샷 존재)에는 적용되지 않는다.
    personas: Vec<InitialPersona>,
    /// 페르소나가 쓸 모델 태그(최대 3). 비면 기본 라우팅. 새 방 시딩에만 적용.
    models: Vec<String>,
}

/// 새 방을 만들 때 프런트가 지정한 초기 참가자 한 명의 축.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitialPersona {
    pub blood: String,
    pub mbti: String,
    pub zodiac: String,
    pub role: String,
}

impl WebStartup {
    pub fn debate(topics: Vec<String>) -> Self {
        Self {
            topics: normalize_topics(topics),
            personas: Vec::new(),
            models: Vec::new(),
        }
    }

    pub fn debate_with_personas(topics: Vec<String>, personas: Vec<InitialPersona>) -> Self {
        Self {
            topics: normalize_topics(topics),
            personas,
            models: Vec::new(),
        }
    }

    /// 페르소나 모델 선택을 설정한다(빌더).
    pub fn with_models(mut self, models: Vec<String>) -> Self {
        self.models = models;
        self
    }

    pub fn topics(&self) -> &[String] {
        &self.topics
    }

    /// 새 방 초기 참가자 스펙(수동). 비어 있으면 호출측이 랜덤 3명을 시딩한다.
    pub fn personas(&self) -> &[InitialPersona] {
        &self.personas
    }

    /// 페르소나가 쓸 모델 태그(최대 3). 비면 기본 라우팅.
    pub fn models(&self) -> &[String] {
        &self.models
    }

    fn opening_prompt(&self) -> Option<String> {
        if self.topics.is_empty() {
            return None;
        }
        Some(format!(
            "토론을 시작합니다. 주제는 '{}'입니다. 첫 발화자는 자기 입장과 근거를 3-5문장으로 분명히 밝히고, 다른 참가자들은 닉네임을 부르며 반박하거나 보완하세요.",
            self.topics.join("', '")
        ))
    }
}

fn normalize_topics(topics: Vec<String>) -> Vec<String> {
    // 여러 주제는 줄바꿈으로 구분한다. 콤마는 토론 주제 문장 안에 흔히 들어가므로
    // (예: "A인가, B인가?") 구분자로 쓰지 않는다.
    topics
        .into_iter()
        .flat_map(|topic| {
            topic
                .split('\n')
                .map(|part| part.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|topic| !topic.is_empty())
        .take(5)
        .collect()
}

fn normalize_room_id(room_id: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in room_id.trim().chars() {
        let normalized = if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            Some(ch.to_ascii_lowercase())
        } else if ch.is_whitespace() || ch == '/' || ch == '\\' || ch == ':' {
            Some('-')
        } else {
            None
        };
        if let Some(ch) = normalized {
            if ch == '-' {
                if last_dash {
                    continue;
                }
                last_dash = true;
            } else {
                last_dash = false;
            }
            out.push(ch);
            if out.len() >= 80 {
                break;
            }
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

fn client_frame_to_cmd(frame: ClientFrame) -> EngineCmd {
    match frame {
        ClientFrame::Message { text } => EngineCmd::Human(text),
        ClientFrame::Topic { topics } => EngineCmd::Topic(topics),
        ClientFrame::Pause { paused } => EngineCmd::SetPaused(paused),
        ClientFrame::Pace { interval_ms } => EngineCmd::SetPace(interval_ms),
        ClientFrame::Invite {
            blood,
            mbti,
            zodiac,
            role,
        } => EngineCmd::Invite {
            blood,
            mbti,
            zodiac,
            role,
        },
        ClientFrame::Remove { id } => EngineCmd::Remove(id),
        ClientFrame::Presence { clients } => EngineCmd::SetClientCount(clients),
        ClientFrame::Reset { topics } => EngineCmd::Reset(topics),
        ClientFrame::HumanProfile {
            blood,
            mbti,
            zodiac,
            role,
        } => EngineCmd::SetHumanProfile {
            blood,
            mbti,
            zodiac,
            role,
        },
    }
}

fn effective_paused(manual_paused: bool, client_count: usize, backend_paused: bool) -> bool {
    manual_paused || client_count == 0 || backend_paused
}

#[cfg(feature = "redis-bus")]
fn make_worker_id(room_id: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{room_id}:{}:{nanos}", std::process::id())
}

/// 방 상태를 RoomStore에 저장한다. 실패는 경고만(비밀 비노출, 크래시 금지).
fn save_room(store: &Option<RoomStore>, session: &LiveSession) {
    if let Some(ref s) = *store {
        if let Err(e) = s.save(
            session.room_id(),
            session.personas(),
            session.persona_meta(),
            &session.state().history,
            session.topics(),
            session.tick_count(),
            session.human_axes(),
            session.report(),
        ) {
            eprintln!("[tunaSalon] rooms.db 저장 실패(비치명): {e}");
        }
    }
}

fn delete_room_storage(room_id: &str) {
    if let Some(path) = RoomStore::default_rooms_db_path() {
        match RoomStore::open(&path).and_then(|store| store.delete_room(room_id)) {
            Ok(()) => {}
            Err(e) => eprintln!("[tunaSalon] rooms.db 방 삭제 실패(비치명): {e}"),
        }
    }
    let mut memory = crate::memory::live_store();
    memory.clear_room(room_id);
}

/// backend 문자열을 모델 이름으로 변환한다.
/// "cloud" -> gemma4:31b-cloud, "friend" -> qwen3.6-35b-fast, 그 외 -> 그대로.
fn backend_to_model(backend: &str) -> String {
    match backend {
        "cloud" => "gemma4:31b-cloud".to_string(),
        "friend" => "qwen3.6-35b-fast".to_string(),
        other => other.to_string(),
    }
}

/// 사람(나) 표시 이름: 4축이 있으면 인디언식 닉네임, 없으면 human_id("나").
fn human_display_name(human_id: &str, axes: Option<&PersonaAxes>) -> String {
    use crate::persona_kit::{indian_name, Blood, Mbti, Zodiac};
    use std::str::FromStr;
    if let Some(a) = axes {
        if let (Ok(m), Ok(b), Ok(z)) = (
            Mbti::from_str(&a.mbti),
            Blood::from_str(&a.blood),
            Zodiac::from_str(&a.zodiac),
        ) {
            return indian_name(m, b, z);
        }
    }
    human_id.to_string()
}

fn build_state(
    session: &LiveSession,
    human_id: &str,
    paused: bool,
    tick_ms: u64,
    reports: &[ReportDto],
) -> ServerFrame {
    let intensities: BTreeMap<String, f64> =
        session.combined_intensities().into_iter().collect();
    let mut participants: Vec<Participant> = session
        .personas()
        .iter()
        .map(|p| {
            let meta = session.persona_meta().get(&p.id);
            let model = meta.map(|m| backend_to_model(&m.backend));
            let axes = meta.and_then(|m| m.axes.as_ref()).map(|a| ParticipantAxes {
                blood: a.blood.clone(),
                mbti: a.mbti.clone(),
                zodiac: a.zodiac.clone(),
                role: a.role.clone(),
            });
            Participant {
                id: p.id.clone(),
                name: p.name.clone(),
                model,
                axes,
            }
        })
        .collect();
    let human_name = human_display_name(human_id, session.human_axes());
    participants.push(Participant {
        id: human_id.to_string(),
        name: human_name.clone(),
        model: None,
        axes: session.human_axes().map(|a| ParticipantAxes {
            blood: a.blood.clone(),
            mbti: a.mbti.clone(),
            zodiac: a.zodiac.clone(),
            role: a.role.clone(),
        }),
    });
    let speaker_name = |speaker: &str| -> String {
        if speaker == human_id {
            return human_name.clone();
        }
        crate::live::persona_display_name(session.personas(), speaker)
    };
    let messages = session
        .state()
        .history
        .iter()
        .filter_map(|event| {
            let content = event.content.as_deref()?.trim();
            if content.is_empty() || event.speaker == crate::live::MODERATOR_SPEAKER {
                return None;
            }
            if event.speaker == human_id
                && content.trim_start().starts_with(crate::debate::DEBATE_OPENING_PREFIX)
            {
                return None;
            }
            Some(HistoryMessage {
                speaker: event.speaker.clone(),
                name: speaker_name(&event.speaker),
                content: content.to_string(),
                ts: event.ts,
            })
        })
        .collect::<Vec<_>>();
    ServerFrame::State {
        room_id: session.room_id().to_string(),
        intensities,
        theta: session.theta(),
        flow: session.flow().map(|f| f.convergence).unwrap_or(0.0),
        mu_scale: session.mu_scale(),
        liveliness: session.liveliness(),
        pending: session.pending_speaker(),
        participants,
        messages,
        topics: session.topics().to_vec(),
        paused,
        tick_ms,
        reports: reports.to_vec(),
    }
}

// 엔진 스레드: blocking LiveSession 구동, frame을 broadcast로 push, cmd를 mpsc로 수신.
fn run_engine(
    mut session: LiveSession,
    human_id: String,
    startup: WebStartup,
    frame_tx: broadcast::Sender<String>,
    mut cmd_rx: mpsc::UnboundedReceiver<EngineCmd>,
    store: Option<RoomStore>,
    #[cfg(feature = "redis-bus")] redis_bus: Option<RedisBusHandle>,
) {
    #[cfg(feature = "redis-bus")]
    let room_id = session.room_id().to_string();
    let room_id_str = session.room_id().to_string();
    let mut cached_reports: Vec<ReportDto> = store
        .as_ref()
        .and_then(|s| s.load_reports(&room_id_str).ok())
        .unwrap_or_default()
        .into_iter()
        .map(ReportDto::from)
        .collect();
    let emit = |tx: &broadcast::Sender<String>, frame: &ServerFrame| {
        if let Ok(json) = serde_json::to_string(frame) {
            #[cfg(feature = "redis-bus")]
            if let Some(ref bus) = redis_bus {
                bus.publish_event_json(&room_id, &json);
            }
            let _ = tx.send(json); // 구독자 없어도 무시(broadcast)
        }
    };

    let mut dirty = false;
    let mut manual_paused = false;
    let mut backend_paused = false;
    let mut client_count = 0usize;
    let mut generation_failures = 0usize;
    // 단계형 토론 종료 대기: tick에서 종료가 확정되면 set, 마지막(클로징) 발화가
    // 도착해 pending이 비면 "토론 마무리" 배너를 1회 보내고 해제한다(발화→배너 순서 보장).
    let mut awaiting_conclusion = false;
    let mut tick_period = Duration::from_millis(DEFAULT_TICK_MS);
    let mut last_state = Instant::now();
    let mut last_tick = Instant::now()
        .checked_sub(tick_period)
        .unwrap_or_else(Instant::now);
    let mut last_save = Instant::now();

    // 새 방에만 startup 주제를 적용한다. 복원된 방(history 존재)은 factory 가 이미
    // topics/phase 를 세팅했으므로, 여기서 set_topics 를 다시 부르면 종료/진행 단계가
    // Opening 으로 덮여 종료 토론이 재진입 시 재실행된다(회귀). report 가 종료 SSOT 라
    // set_topics 자체도 Concluded 를 보존하지만(이중 안전), 복원 방엔 애초에 재적용하지 않는다.
    if session.state().history.is_empty() && !startup.topics().is_empty() {
        session.set_topics(startup.topics().to_vec());
        emit(
            &frame_tx,
            &ServerFrame::System {
                text: format!("토론 주제: {}", startup.topics().join(", ")),
            },
        );
        if let Some(text) = startup.opening_prompt() {
            emit(&frame_tx, &ServerFrame::System { text });
        }
        dirty = true;
    }

    // 초기 state 1회
    emit(
        &frame_tx,
        &build_state(
            &session,
            &human_id,
            effective_paused(manual_paused, client_count, backend_paused),
            tick_period.as_millis() as u64,
            &cached_reports,
        ),
    );

    loop {
        // 1. 명령 처리(즉시 반응)
        while let Ok(cmd) = cmd_rx.try_recv() {
            let paused = effective_paused(manual_paused, client_count, backend_paused);
            match cmd {
                EngineCmd::Human(text) => {
                    session.submit_human(text.clone());
                    let ts = session.state().history.last().map(|e| e.ts).unwrap_or(0.0);
                    emit(
                        &frame_tx,
                        &ServerFrame::Utterance {
                            speaker: human_id.clone(),
                            name: human_display_name(&human_id, session.human_axes()),
                            content: text,
                            ts,
                        },
                    );
                    emit(
                        &frame_tx,
                        &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                    ); // λ 스파이크 즉시 반영
                    dirty = true;
                }
                EngineCmd::Topic(topics) => {
                    session.set_topics(topics.clone());
                    if let Some(first) = topics.first() {
                        emit(
                            &frame_tx,
                            &ServerFrame::System {
                                text: format!("화제가 '{first}'로 바뀌었습니다"),
                            },
                        );
                    }
                    emit(
                        &frame_tx,
                        &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                    );
                    dirty = true;
                }
                EngineCmd::Reset(topics) => {
                    let normalized = normalize_topics(topics);
                    let room_id = session.room_id().to_string();
                    session.reset_discussion(normalized.clone());
                    if let Some(ref store) = store {
                        if let Err(e) = store.delete_room(&room_id) {
                            eprintln!("[tunaSalon] rooms.db 방 초기화 실패(비치명): {e}");
                        }
                    }
                    emit(
                        &frame_tx,
                        &ServerFrame::System {
                            text: "토론을 초기화했습니다".to_string(),
                        },
                    );
                    if !normalized.is_empty() {
                        emit(
                            &frame_tx,
                            &ServerFrame::System {
                                text: format!("토론 주제: {}", normalized.join(", ")),
                            },
                        );
                        let startup = WebStartup::debate(normalized);
                        if let Some(text) = startup.opening_prompt() {
                            emit(&frame_tx, &ServerFrame::System { text });
                        }
                    }
                    generation_failures = 0;
                    backend_paused = false;
                    dirty = true;
                    let paused = effective_paused(manual_paused, client_count, backend_paused);
                    emit(
                        &frame_tx,
                        &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                    );
                }
                EngineCmd::SetPaused(p) => {
                    let was_paused = effective_paused(manual_paused, client_count, backend_paused);
                    manual_paused = p;
                    let paused = effective_paused(manual_paused, client_count, backend_paused);
                    if paused && !was_paused {
                        if session.cancel_pending_generation() {
                            dirty = true;
                        }
                    }
                    emit(
                        &frame_tx,
                        &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                    ); // 즉시 paused 상태 반영
                }
                EngineCmd::SetClientCount(count) => {
                    let was_paused = effective_paused(manual_paused, client_count, backend_paused);
                    let prev = client_count;
                    client_count = count;
                    let paused = effective_paused(manual_paused, client_count, backend_paused);
                    if paused && !was_paused {
                        if session.cancel_pending_generation() {
                            dirty = true;
                        }
                    }
                    // 새 클라이언트가 붙었고(0→1+) 결론 리포트가 있으면 다시 보여준다(재접속 재표시).
                    if prev == 0 && count > 0 {
                        if let Some(report) = session.report() {
                            emit(
                                &frame_tx,
                                &ServerFrame::Report {
                                    text: report.to_string(),
                                },
                            );
                        }
                    }
                    emit(
                        &frame_tx,
                        &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                    );
                }
                EngineCmd::SetPace(ms) => {
                    tick_period = Duration::from_millis(ms.clamp(1500, 12000));
                    emit(
                        &frame_tx,
                        &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                    ); // 즉시 반영
                }
                EngineCmd::SetHumanProfile {
                    blood,
                    mbti,
                    zodiac,
                    role,
                } => {
                    session.set_human_axes(Some(PersonaAxes {
                        blood,
                        mbti,
                        zodiac,
                        role,
                    }));
                    // 즉시 영속(재시작·재접속 후에도 내 캐릭터 유지).
                    save_room(&store, &session);
                    dirty = false;
                    emit(
                        &frame_tx,
                        &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                    );
                }
                EngineCmd::Remove(id) => {
                    let name = session
                        .personas()
                        .iter()
                        .find(|p| p.id == id)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| id.clone());
                    session.remove_persona(&id);
                    emit(
                        &frame_tx,
                        &ServerFrame::System {
                            text: format!("{name}님이 나갔습니다"),
                        },
                    );
                    emit(
                        &frame_tx,
                        &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                    );
                    dirty = true;
                }
                EngineCmd::Invite {
                    blood,
                    mbti,
                    zodiac,
                    role,
                } => {
                    // 인원 제한: 최대 3명
                    if session.personas().len() >= 3 {
                        emit(
                            &frame_tx,
                            &ServerFrame::System {
                                text: "방이 가득 찼습니다(최대 3명). 먼저 내보내세요".to_string(),
                            },
                        );
                        continue;
                    }
                    // 파싱
                    let parsed_blood = match Blood::from_str(&blood) {
                        Ok(v) => v,
                        Err(_) => {
                            emit(
                                &frame_tx,
                                &ServerFrame::System {
                                    text: format!("초대 실패: 잘못된 혈액형 '{blood}'"),
                                },
                            );
                            continue;
                        }
                    };
                    let parsed_mbti = match Mbti::from_str(&mbti) {
                        Ok(v) => v,
                        Err(_) => {
                            emit(
                                &frame_tx,
                                &ServerFrame::System {
                                    text: format!("초대 실패: 잘못된 MBTI '{mbti}'"),
                                },
                            );
                            continue;
                        }
                    };
                    let parsed_zodiac = match Zodiac::from_str(&zodiac) {
                        Ok(v) => v,
                        Err(_) => {
                            emit(
                                &frame_tx,
                                &ServerFrame::System {
                                    text: format!("초대 실패: 잘못된 별자리 '{zodiac}'"),
                                },
                            );
                            continue;
                        }
                    };
                    // 역할은 잠정 폐기라 개성엔 안 쓰이지만, 입력 검증(잘못된 역할 거부)은 유지.
                    let _parsed_role = match role {
                        Some(ref r) => match Role::from_str(r) {
                            Ok(v) => v,
                            Err(_) => {
                                emit(
                                    &frame_tx,
                                    &ServerFrame::System {
                                        text: format!("초대 실패: 잘못된 역할 '{r}'"),
                                    },
                                );
                                continue;
                            }
                        },
                        None => Role::all()[0],
                    };
                    // 조립(역할 잠정 폐기: 개성은 혈액형+별자리+MBTI만, role은 axes/아바타용).
                    let assembled =
                        assemble_roleless(parsed_mbti, parsed_blood, parsed_zodiac, "");
                    // id 충돌 확인
                    if session.persona_meta().contains_key(&assembled.persona.id)
                        || session
                            .personas()
                            .iter()
                            .any(|p| p.id == assembled.persona.id)
                    {
                        emit(
                            &frame_tx,
                            &ServerFrame::System {
                                text: "이미 같은 조합의 참가자가 있습니다".to_string(),
                            },
                        );
                        continue;
                    }
                    // 자동 backend 배분: cloud 1명 우선, 나머지 friend.
                    // 단 friend 백엔드가 실제 가용할 때만 friend로 보낸다 — 서버 다운(cloud-only)
                    // 상태에서 friend로 라우팅하면 그 참가자가 발화하지 못한다(침묵 버그).
                    let cloud_count = session
                        .persona_meta()
                        .values()
                        .filter(|m| m.backend == "cloud")
                        .count();
                    let backend = if session.has_backend("friend") && cloud_count >= 1 {
                        "friend".to_string()
                    } else {
                        "cloud".to_string()
                    };
                    let name = assembled.persona.name.clone();
                    session.add_persona(
                        assembled.persona.clone(),
                        PersonaMeta {
                            backend,
                            system_prompt: assembled.system_prompt,
                            modifier: assembled.modifier,
                            axes: Some(PersonaAxes {
                                blood: blood.clone(),
                                mbti: mbti.clone(),
                                zodiac: zodiac.clone(),
                                role: role.clone().unwrap_or_else(|| "friend".to_string()),
                            }),
                        },
                    );
                    emit(
                        &frame_tx,
                        &ServerFrame::System {
                            text: format!("{name}님이 입장했습니다"),
                        },
                    );
                    emit(
                        &frame_tx,
                        &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                    );
                    dirty = true;
                }
                EngineCmd::DeleteAndShutdown => {
                    let room_id = session.room_id().to_string();
                    session.reset_discussion(vec![]);
                    if let Some(ref store) = store {
                        if let Err(e) = store.delete_room(&room_id) {
                            eprintln!("[tunaSalon] rooms.db 방 삭제 실패(비치명): {e}");
                        }
                    }
                    session.shutdown();
                    return;
                }
                EngineCmd::Shutdown => {
                    save_room(&store, &session);
                    session.shutdown();
                    return;
                }
            }
        }

        // 2. tick (주기) - paused면 skip
        let paused = effective_paused(manual_paused, client_count, backend_paused);
        if !paused && last_tick.elapsed() >= tick_period {
            session.tick();
            if session.take_just_concluded() {
                awaiting_conclusion = true;
            }
            last_tick = Instant::now();
        }

        // 3. 완성 발화 drain -> utterance frame
        while let Some(ev) = session.poll_generation() {
            if let Some(content) = ev.content {
                generation_failures = 0;
                let name = session
                    .personas()
                    .iter()
                    .find(|p| p.id == ev.speaker)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| ev.speaker.clone());
                emit(
                    &frame_tx,
                    &ServerFrame::Utterance {
                        speaker: ev.speaker,
                        name,
                        content,
                        ts: ev.ts,
                    },
                );
                dirty = true;
            } else {
                generation_failures += 1;
                let name = session
                    .personas()
                    .iter()
                    .find(|p| p.id == ev.speaker)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| ev.speaker.clone());
                let text = if generation_failures >= 3 {
                    backend_paused = true;
                    format!(
                        "{name} 발화 생성 실패: LLM 백엔드가 응답하지 않아 방을 일시정지했습니다. Ollama(localhost:11434) 또는 friend 서버 상태를 확인하세요."
                    )
                } else {
                    format!(
                        "{name} 발화 생성 실패: LLM 백엔드가 응답하지 않았습니다. 다음 발화를 다시 시도합니다."
                    )
                };
                emit(&frame_tx, &ServerFrame::System { text });
                let paused = effective_paused(manual_paused, client_count, backend_paused);
                emit(
                    &frame_tx,
                    &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
                );
            }
        }

        // 3.5 단계형 토론 종료: 클로징 발화 도착(pending 해제) 후 1회 — 배너 + 메타 리포트.
        if awaiting_conclusion && !session.is_pending() {
            emit(
                &frame_tx,
                &ServerFrame::System {
                    text: "토론이 마무리됐습니다. 정리 리포트를 작성합니다… (더 이어가려면 메시지를 입력하세요)".to_string(),
                },
            );
            awaiting_conclusion = false;
            dirty = true;
            // 메타 분석가 리포트(블로킹 ~수초, 방이 idle이라 허용). 실패하면 배너만.
            // 생성한 리포트는 세션에 저장하고 rooms.db에 영속(재접속 재표시·로비 요약용).
            let past_conclusions = store
                .as_ref()
                .and_then(|s| s.recent_conclusions(&room_id_str, 2).ok())
                .unwrap_or_default();
            if let Some(report) = session.summarize_debate(&past_conclusions) {
                let conclusion = crate::debate::report::extract_conclusion_section(&report);
                let topic = session.topics().join(", ");
                if let Some(ref s) = store {
                    if let Ok(seq) = s.append_report(&room_id_str, &topic, &report, &conclusion) {
                        let created_at = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs() as i64)
                            .unwrap_or(0);
                        cached_reports.push(
                            crate::roomstore::ReportRecord {
                                seq,
                                created_at,
                                topic,
                                markdown: report.clone(),
                                conclusion,
                            }
                            .into(),
                        );
                    }
                }
                session.set_report(Some(report.clone()));
                save_room(&store, &session);
                emit(&frame_tx, &ServerFrame::Report { text: report });
            }
            let paused = effective_paused(manual_paused, client_count, backend_paused);
            emit(
                &frame_tx,
                &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
            );
        }

        // 4. state frame (주기)
        if last_state.elapsed() >= STATE_PERIOD {
            let paused = effective_paused(manual_paused, client_count, backend_paused);
            emit(
                &frame_tx,
                &build_state(&session, &human_id, paused, tick_period.as_millis() as u64, &cached_reports),
            );
            last_state = Instant::now();
        }

        // 5. 주기 저장 (dirty && SAVE_PERIOD 경과)
        if dirty && last_save.elapsed() >= SAVE_PERIOD {
            save_room(&store, &session);
            dirty = false;
            last_save = Instant::now();
        }

        std::thread::sleep(POLL_PERIOD);
    }
}

#[cfg(feature = "redis-bus")]
fn spawn_redis_command_reader(
    bus: RedisBus,
    room_id: String,
    cmd_tx: mpsc::UnboundedSender<EngineCmd>,
) {
    tokio::spawn(async move {
        let mut last_id = match bus.command_cursor(&room_id).await {
            Ok(Some(id)) => id,
            Ok(None) => "$".to_string(),
            Err(e) => {
                eprintln!("[tunaSalon] Redis command cursor read failed; starting at '$': {e}");
                "$".to_string()
            }
        };
        eprintln!("[tunaSalon] Redis command reader started at id '{last_id}'");
        loop {
            match bus.read_commands(&room_id, &last_id, 5_000, 100).await {
                Ok(messages) => {
                    for message in messages {
                        last_id = message.id;
                        match serde_json::from_str::<ClientFrame>(&message.payload) {
                            Ok(frame) => {
                                if cmd_tx.send(client_frame_to_cmd(frame)).is_err() {
                                    return;
                                }
                            }
                            Err(e) => {
                                eprintln!("[tunaSalon] Redis command decode failed: {e}");
                            }
                        }
                        if let Err(e) = bus.mark_command_consumed(&room_id, &last_id).await {
                            eprintln!("[tunaSalon] Redis command cursor write failed: {e}");
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[tunaSalon] Redis command read failed: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    });
}

#[cfg(feature = "redis-bus")]
fn spawn_redis_event_subscriber(
    bus: RedisBus,
    room_id: String,
    frame_tx: broadcast::Sender<String>,
) {
    tokio::spawn(async move {
        if let Err(e) = bus.subscribe_events(&room_id, frame_tx).await {
            eprintln!("[tunaSalon] Redis event subscribe stopped: {e}");
        }
    });
}

#[cfg(feature = "redis-bus")]
fn spawn_owner_refresher(
    bus: RedisBus,
    room_id: String,
    worker_id: String,
    cmd_tx: mpsc::UnboundedSender<EngineCmd>,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(OWNER_REFRESH_SECS)).await;
            match bus
                .refresh_owner(&room_id, &worker_id, OWNER_TTL_SECS)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    eprintln!("[tunaSalon] Redis owner lease lost for room '{room_id}'");
                    let _ = cmd_tx.send(EngineCmd::Shutdown);
                    return;
                }
                Err(e) => {
                    eprintln!("[tunaSalon] Redis owner refresh failed: {e}");
                }
            }
        }
    });
}

pub type WebSessionFactory =
    Arc<dyn Fn(String, &WebStartup) -> (LiveSession, Option<RoomStore>) + Send + Sync + 'static>;

struct RoomRuntime {
    room_id: String,
    frame_tx: broadcast::Sender<String>,
    cmd_tx: mpsc::UnboundedSender<EngineCmd>,
    client_count: Arc<AtomicUsize>,
    #[cfg(feature = "redis-bus")]
    redis_bus: Option<RedisBusHandle>,
}

#[derive(Clone)]
struct MultiAppState {
    default_room_id: String,
    default_startup: WebStartup,
    rooms: Arc<Mutex<HashMap<String, Arc<RoomRuntime>>>>,
    factory: WebSessionFactory,
    /// 로비 추천 토론 주제(12h마다 백그라운드 갱신). 비면 프런트가 정적 폴백.
    topics_cache: Arc<Mutex<Option<Vec<crate::lobby_topics::CategoryTopics>>>>,
    #[cfg(feature = "redis-bus")]
    redis_bus: Option<RedisBus>,
}

async fn spawn_room_runtime(
    room_id: String,
    startup: WebStartup,
    factory: WebSessionFactory,
    #[cfg(feature = "redis-bus")] redis_bus: Option<RedisBus>,
) -> Arc<RoomRuntime> {
    let (session, store) = factory(room_id.clone(), &startup);
    let human_id = "나".to_string();
    let (frame_tx, _) = broadcast::channel::<String>(256);
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<EngineCmd>();

    #[cfg(feature = "redis-bus")]
    let mut room_redis_bus = redis_bus;
    #[cfg(feature = "redis-bus")]
    let owner_worker_id = if let Some(ref bus) = room_redis_bus {
        let worker_id = make_worker_id(&room_id);
        match bus
            .try_acquire_owner(&room_id, &worker_id, OWNER_TTL_SECS)
            .await
        {
            Ok(true) => {
                eprintln!(
                    "[tunaSalon] Redis room owner acquired: room='{room_id}' worker='{worker_id}'"
                );
                Some(worker_id)
            }
            Ok(false) => {
                eprintln!(
                    "[tunaSalon] Redis room owner exists; running as gateway for room '{room_id}'"
                );
                None
            }
            Err(e) => {
                eprintln!(
                    "[tunaSalon] Redis owner acquisition failed; room '{room_id}' local-only: {e}"
                );
                room_redis_bus = None;
                None
            }
        }
    } else {
        None
    };
    #[cfg(feature = "redis-bus")]
    let redis_writer = room_redis_bus.clone().map(RedisBusHandle::spawn);
    #[cfg(feature = "redis-bus")]
    let start_engine = room_redis_bus.is_none() || owner_worker_id.is_some();
    #[cfg(not(feature = "redis-bus"))]
    let start_engine = true;

    #[cfg(feature = "redis-bus")]
    if let Some(ref bus) = room_redis_bus {
        if let Some(ref worker_id) = owner_worker_id {
            spawn_redis_command_reader(bus.clone(), room_id.clone(), cmd_tx.clone());
            spawn_owner_refresher(
                bus.clone(),
                room_id.clone(),
                worker_id.clone(),
                cmd_tx.clone(),
            );
        } else {
            spawn_redis_event_subscriber(bus.clone(), room_id.clone(), frame_tx.clone());
        }
    }

    if start_engine {
        let frame_tx_engine = frame_tx.clone();
        #[cfg(feature = "redis-bus")]
        let redis_bus_engine = redis_writer.clone();
        std::thread::spawn(move || {
            run_engine(
                session,
                human_id,
                startup,
                frame_tx_engine,
                cmd_rx,
                store,
                #[cfg(feature = "redis-bus")]
                redis_bus_engine,
            );
        });
    } else {
        drop(cmd_rx);
    }

    Arc::new(RoomRuntime {
        room_id,
        frame_tx,
        cmd_tx,
        client_count: Arc::new(AtomicUsize::new(0)),
        #[cfg(feature = "redis-bus")]
        redis_bus: redis_writer,
    })
}

async fn get_room_runtime(
    st: &MultiAppState,
    room_id: String,
    startup: WebStartup,
) -> Arc<RoomRuntime> {
    let mut rooms = st.rooms.lock().await;
    if let Some(runtime) = rooms.get(&room_id) {
        return runtime.clone();
    }
    let runtime = spawn_room_runtime(
        room_id.clone(),
        startup,
        st.factory.clone(),
        #[cfg(feature = "redis-bus")]
        st.redis_bus.clone(),
    )
    .await;
    rooms.insert(room_id, runtime.clone());
    runtime
}

#[derive(Deserialize, Default)]
struct WsParams {
    room_id: Option<String>,
    topic: Option<String>,
    /// 새 방 초기 참가자(수동 구성). "blood:mbti:zodiac:role" 를 ';'로 구분해 최대 3명.
    /// 비거나 없으면 호출측이 랜덤 3명을 시딩한다.
    personas: Option<String>,
    /// 페르소나가 쓸 LLM 모델 태그 ','로 구분(최대 3). CLOUD_MODELS의 값만 허용.
    /// 비면 기본 라우팅(gemma/friend).
    models: Option<String>,
}

/// `models` 쿼리(",")를 파싱한다. CLOUD_MODELS에 있는 태그만, 최대 3개, 중복 제거.
fn parse_models_param(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for tag in raw.split(',').map(|s| s.trim()) {
        if crate::model::CLOUD_MODELS.contains(&tag) && !out.iter().any(|m| m == tag) {
            out.push(tag.to_string());
        }
        if out.len() == 3 {
            break;
        }
    }
    out
}

/// `WsParams.personas` 쿼리("blood:mbti:zodiac:role;..." )를 파싱한다. 잘못된 항목은 건너뛴다.
fn parse_persona_param(raw: Option<&str>) -> Vec<InitialPersona> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    raw.split(';')
        .filter_map(|entry| {
            let parts: Vec<&str> = entry.split(':').map(|s| s.trim()).collect();
            if parts.len() != 4 || parts.iter().any(|p| p.is_empty()) {
                return None;
            }
            Some(InitialPersona {
                blood: parts[0].to_string(),
                mbti: parts[1].to_string(),
                zodiac: parts[2].to_string(),
                role: parts[3].to_string(),
            })
        })
        .take(3)
        .collect()
}

async fn handle_runtime_socket(socket: axum::extract::ws::WebSocket, runtime: Arc<RoomRuntime>) {
    use axum::extract::ws::Message;
    use futures_util::{SinkExt, StreamExt};

    let (mut sender, mut receiver) = socket.split();
    let mut frame_rx = runtime.frame_tx.subscribe();

    let connected = runtime.client_count.fetch_add(1, Ordering::SeqCst) + 1;
    #[cfg(feature = "redis-bus")]
    {
        if let Some(ref bus) = runtime.redis_bus {
            let presence = format!(r#"{{"type":"presence","clients":{connected}}}"#);
            bus.submit_command_json(&runtime.room_id, &presence);
        } else {
            let _ = runtime.cmd_tx.send(EngineCmd::SetClientCount(connected));
        }
    }
    #[cfg(not(feature = "redis-bus"))]
    {
        let _ = runtime.cmd_tx.send(EngineCmd::SetClientCount(connected));
    }

    let send_task = tokio::spawn(async move {
        loop {
            match frame_rx.recv().await {
                Ok(json) => {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    let cmd_tx = runtime.cmd_tx.clone();
    while let Some(Ok(msg)) = receiver.next().await {
        if let Message::Text(txt) = msg {
            if let Ok(frame) = serde_json::from_str::<ClientFrame>(&txt) {
                #[cfg(feature = "redis-bus")]
                if let Some(ref bus) = runtime.redis_bus {
                    bus.submit_command_json(&runtime.room_id, &txt);
                    continue;
                }
                let _ = cmd_tx.send(client_frame_to_cmd(frame));
            }
        }
    }
    send_task.abort();

    let disconnected = runtime
        .client_count
        .fetch_sub(1, Ordering::SeqCst)
        .saturating_sub(1);
    #[cfg(feature = "redis-bus")]
    {
        if let Some(ref bus) = runtime.redis_bus {
            let presence = format!(r#"{{"type":"presence","clients":{disconnected}}}"#);
            bus.submit_command_json(&runtime.room_id, &presence);
        } else {
            let _ = runtime.cmd_tx.send(EngineCmd::SetClientCount(disconnected));
        }
    }
    #[cfg(not(feature = "redis-bus"))]
    {
        let _ = runtime.cmd_tx.send(EngineCmd::SetClientCount(disconnected));
    }
}

pub fn serve_multi(
    host: &str,
    port: u16,
    default_room_id: String,
    default_startup: WebStartup,
    factory: WebSessionFactory,
) {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[tunaSalon] web: tokio runtime 생성 실패: {e}");
            return;
        }
    };
    rt.block_on(async move {
        use axum::extract::ws::WebSocketUpgrade;
        use axum::extract::{Path, Query, State as AxumState};
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        use axum::{
            routing::{delete, get},
            Router,
        };
        use tower_http::services::{ServeDir, ServeFile};

        #[cfg(feature = "redis-bus")]
        let redis_bus = RedisBus::open_from_env();

        // 로비 추천 주제: 서버 시작 시 1회 + 12h마다 백그라운드로 웹서치+gemma 생성.
        // 블로킹 호출(reqwest)이라 spawn_blocking으로 감싼다. 실패하면 캐시는 비어 프런트가 정적 폴백.
        let topics_cache: Arc<Mutex<Option<Vec<crate::lobby_topics::CategoryTopics>>>> =
            Arc::new(Mutex::new(None));
        {
            let cache = topics_cache.clone();
            tokio::spawn(async move {
                loop {
                    let generated = tokio::task::spawn_blocking(
                        crate::lobby_topics::generate_suggested_topics,
                    )
                    .await
                    .ok()
                    .flatten();
                    match generated {
                        Some(topics) => {
                            eprintln!("[tunaSalon] 로비 추천 주제 {} 분야 생성", topics.len());
                            *cache.lock().await = Some(topics);
                        }
                        None => eprintln!(
                            "[tunaSalon] 로비 추천 주제 생성 실패(키/네트워크) — 정적 폴백 사용"
                        ),
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(12 * 60 * 60)).await;
                }
            });
        }

        let app_state = MultiAppState {
            default_room_id: normalize_room_id(&default_room_id, "salon"),
            default_startup,
            rooms: Arc::new(Mutex::new(HashMap::new())),
            factory,
            topics_cache,
            #[cfg(feature = "redis-bus")]
            redis_bus,
        };

        async fn ws_handler(
            ws: WebSocketUpgrade,
            Query(params): Query<WsParams>,
            AxumState(st): AxumState<MultiAppState>,
        ) -> impl IntoResponse {
            let room_id = normalize_room_id(
                params.room_id.as_deref().unwrap_or(&st.default_room_id),
                &st.default_room_id,
            );
            let topics = normalize_topics(params.topic.into_iter().collect());
            let personas = parse_persona_param(params.personas.as_deref());
            let models = parse_models_param(params.models.as_deref());
            let startup = if topics.is_empty()
                && personas.is_empty()
                && models.is_empty()
                && room_id == st.default_room_id
            {
                st.default_startup.clone()
            } else {
                WebStartup::debate_with_personas(topics, personas).with_models(models)
            };
            let runtime = get_room_runtime(&st, room_id, startup).await;
            ws.on_upgrade(move |socket| handle_runtime_socket(socket, runtime))
        }

        async fn delete_room_handler(
            Path(raw_room_id): Path<String>,
            AxumState(st): AxumState<MultiAppState>,
        ) -> impl IntoResponse {
            let room_id = normalize_room_id(&raw_room_id, &st.default_room_id);
            if let Some(runtime) = st.rooms.lock().await.remove(&room_id) {
                let _ = runtime.cmd_tx.send(EngineCmd::DeleteAndShutdown);
            }
            delete_room_storage(&room_id);
            StatusCode::NO_CONTENT
        }

        // 로비 추천 토론 주제(분야별). 캐시 비었으면 빈 배열 → 프런트가 정적 폴백.
        async fn suggested_topics_handler(
            AxumState(st): AxumState<MultiAppState>,
        ) -> impl IntoResponse {
            let topics = st.topics_cache.lock().await.clone().unwrap_or_default();
            axum::Json(topics)
        }

        // 방 리포트 조회 — 서버가 배지 권위. concluded=true 면 로비 카드에 배지 표시.
        async fn room_report_handler(
            Path(raw_room_id): Path<String>,
            AxumState(st): AxumState<MultiAppState>,
        ) -> impl IntoResponse {
            let room_id = normalize_room_id(&raw_room_id, &st.default_room_id);
            let reports = RoomStore::default_rooms_db_path()
                .and_then(|p| RoomStore::open(&p).ok())
                .and_then(|store| store.load_reports(&room_id).ok())
                .unwrap_or_default();
            let concluded = !reports.is_empty();
            let summary = reports.last().map(|r| r.conclusion.clone()).unwrap_or_default();
            axum::Json(RoomReportResponse { concluded, summary })
        }

        // GET /api/rooms - 영속된 모든 방을 최신순으로("이전 토론" 페이지용).
        async fn rooms_list_handler() -> impl IntoResponse {
            let rooms: Vec<RoomListItemDto> = RoomStore::default_rooms_db_path()
                .and_then(|p| RoomStore::open(&p).ok())
                .and_then(|store| store.list_rooms().ok())
                .unwrap_or_default()
                .into_iter()
                .map(RoomListItemDto::from)
                .collect();
            axum::Json(rooms)
        }

        let dist_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist");
        let index_file = concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist/index.html");
        if !std::path::Path::new(index_file).exists() {
            eprintln!(
                "[tunaSalon] web: 정적 산출물이 없습니다 ({index_file}).\n\
                 먼저 `cd web && pnpm install && pnpm build` 로 web/dist 를 생성하세요."
            );
        } else {
            eprintln!("[tunaSalon] web: 정적 서빙 {dist_dir}");
        }

        let serve_dir = ServeDir::new(dist_dir)
            .append_index_html_on_directories(true)
            .not_found_service(ServeFile::new(index_file));
        let app = Router::new()
            .route("/ws", get(ws_handler))
            .route("/api/rooms", get(rooms_list_handler))
            .route("/api/rooms/{room_id}", delete(delete_room_handler))
            .route("/api/rooms/{room_id}/report", get(room_report_handler))
            .route("/api/suggested-topics", get(suggested_topics_handler))
            .fallback_service(serve_dir)
            .with_state(app_state);

        let addr = format!("{host}:{port}");
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[tunaSalon] web: {addr} 바인드 실패: {e}");
                return;
            }
        };
        eprintln!("[tunaSalon] web 서버: http://{addr} (multi-room, LAN 접속 가능, /ws WebSocket)");
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("[tunaSalon] web serve 오류: {e}");
        }
    });
}

// axum 라우터 + serve. main에서 호출(blocking, 내부에서 tokio runtime).
pub fn serve(
    host: &str,
    port: u16,
    session: LiveSession,
    human_id: String,
    startup: WebStartup,
    store: Option<RoomStore>,
) {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[tunaSalon] web: tokio runtime 생성 실패: {e}");
            return;
        }
    };
    rt.block_on(async move {
        #[cfg(feature = "redis-bus")]
        let room_id = session.room_id().to_string();
        let (frame_tx, _) = broadcast::channel::<String>(256);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<EngineCmd>();
        #[cfg(feature = "redis-bus")]
        let mut redis_bus = RedisBus::open_from_env();
        #[cfg(feature = "redis-bus")]
        let owner_worker_id = if let Some(ref bus) = redis_bus {
            let worker_id = make_worker_id(&room_id);
            match bus.try_acquire_owner(&room_id, &worker_id, OWNER_TTL_SECS).await {
                Ok(true) => {
                    eprintln!(
                        "[tunaSalon] Redis room owner acquired: room='{room_id}' worker='{worker_id}'"
                    );
                    Some(worker_id)
                }
                Ok(false) => {
                    eprintln!("[tunaSalon] Redis room owner exists; running as gateway for room '{room_id}'");
                    None
                }
                Err(e) => {
                    eprintln!(
                        "[tunaSalon] Redis owner acquisition failed; running local-only owner: {e}"
                    );
                    redis_bus = None;
                    None
                }
            }
        } else {
            None
        };
        #[cfg(feature = "redis-bus")]
        let redis_writer = redis_bus.clone().map(RedisBusHandle::spawn);
        #[cfg(feature = "redis-bus")]
        let start_engine = redis_bus.is_none() || owner_worker_id.is_some();
        #[cfg(not(feature = "redis-bus"))]
        let start_engine = true;

        // 엔진 전용 스레드(blocking)
        #[cfg(feature = "redis-bus")]
        if let Some(ref bus) = redis_bus {
            if let Some(ref worker_id) = owner_worker_id {
                spawn_redis_command_reader(bus.clone(), room_id.clone(), cmd_tx.clone());
                spawn_owner_refresher(
                    bus.clone(),
                    room_id.clone(),
                    worker_id.clone(),
                    cmd_tx.clone(),
                );
            } else {
                spawn_redis_event_subscriber(bus.clone(), room_id.clone(), frame_tx.clone());
            }
        }
        let engine_handle = if start_engine {
            let frame_tx_engine = frame_tx.clone();
            #[cfg(feature = "redis-bus")]
            let redis_bus_engine = redis_writer.clone();
            Some(std::thread::spawn(move || {
                run_engine(
                    session,
                    human_id,
                    startup,
                    frame_tx_engine,
                    cmd_rx,
                    store,
                    #[cfg(feature = "redis-bus")]
                    redis_bus_engine,
                );
            }))
        } else {
            drop(cmd_rx);
            None
        };

        use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
        use axum::extract::State as AxumState;
        use axum::response::IntoResponse;
        use axum::{routing::get, Router};
        use tower_http::services::{ServeDir, ServeFile};

        #[derive(Clone)]
        struct AppState {
            #[cfg(feature = "redis-bus")]
            room_id: String,
            frame_tx: broadcast::Sender<String>,
            cmd_tx: mpsc::UnboundedSender<EngineCmd>,
            client_count: Arc<AtomicUsize>,
            #[cfg(feature = "redis-bus")]
            redis_bus: Option<RedisBusHandle>,
        }

        async fn ws_handler(
            ws: WebSocketUpgrade,
            AxumState(st): AxumState<AppState>,
        ) -> impl IntoResponse {
            ws.on_upgrade(move |socket| handle_socket(socket, st))
        }

        async fn handle_socket(socket: WebSocket, st: AppState) {
            use futures_util::{SinkExt, StreamExt};
            let (mut sender, mut receiver) = socket.split();
            let mut frame_rx = st.frame_tx.subscribe();

            let connected = st.client_count.fetch_add(1, Ordering::SeqCst) + 1;
            #[cfg(feature = "redis-bus")]
            {
                if let Some(ref bus) = st.redis_bus {
                    let presence = format!(r#"{{"type":"presence","clients":{connected}}}"#);
                    bus.submit_command_json(&st.room_id, &presence);
                } else {
                    let _ = st.cmd_tx.send(EngineCmd::SetClientCount(connected));
                }
            }
            #[cfg(not(feature = "redis-bus"))]
            {
                let _ = st.cmd_tx.send(EngineCmd::SetClientCount(connected));
            }

            // 서버->클라: broadcast -> ws
            let send_task = tokio::spawn(async move {
                loop {
                    match frame_rx.recv().await {
                        Ok(json) => {
                            if sender
                                .send(Message::Text(json.into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(_) => break,
                    }
                }
            });

            // 클라->서버: ws -> cmd
            let cmd_tx = st.cmd_tx.clone();
            while let Some(Ok(msg)) = receiver.next().await {
                if let Message::Text(txt) = msg {
                    if let Ok(frame) = serde_json::from_str::<ClientFrame>(&txt) {
                        #[cfg(feature = "redis-bus")]
                        if let Some(ref bus) = st.redis_bus {
                            bus.submit_command_json(&st.room_id, &txt);
                            continue;
                        }
                        let _ = cmd_tx.send(client_frame_to_cmd(frame));
                    }
                }
            }
            send_task.abort();

            let disconnected = st
                .client_count
                .fetch_sub(1, Ordering::SeqCst)
                .saturating_sub(1);
            #[cfg(feature = "redis-bus")]
            {
                if let Some(ref bus) = st.redis_bus {
                    let presence = format!(r#"{{"type":"presence","clients":{disconnected}}}"#);
                    bus.submit_command_json(&st.room_id, &presence);
                } else {
                    let _ = st.cmd_tx.send(EngineCmd::SetClientCount(disconnected));
                }
            }
            #[cfg(not(feature = "redis-bus"))]
            {
                let _ = st.cmd_tx.send(EngineCmd::SetClientCount(disconnected));
            }
        }

        let app_state = AppState {
            #[cfg(feature = "redis-bus")]
            room_id: room_id.clone(),
            frame_tx: frame_tx.clone(),
            cmd_tx: cmd_tx.clone(),
            client_count: Arc::new(AtomicUsize::new(0)),
            #[cfg(feature = "redis-bus")]
            redis_bus: redis_writer,
        };

        // 정적 산출물 경로: cwd 의존을 피해 컴파일 시점의 repo 경로(CARGO_MANIFEST_DIR) 기준 절대경로.
        // (어느 디렉터리에서 실행하든 <repo>/web/dist 를 서빙)
        let dist_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist");
        let index_file = concat!(env!("CARGO_MANIFEST_DIR"), "/web/dist/index.html");
        if !std::path::Path::new(index_file).exists() {
            eprintln!(
                "[tunaSalon] web: 정적 산출물이 없습니다 ({index_file}).\n\
                 먼저 `cd web && pnpm install && pnpm build` 로 web/dist 를 생성하세요."
            );
        } else {
            eprintln!("[tunaSalon] web: 정적 서빙 {dist_dir}");
        }
        // SPA fallback: 없는 경로는 index.html 로(클라이언트 라우팅 대비).
        let serve_dir = ServeDir::new(dist_dir)
            .append_index_html_on_directories(true)
            .not_found_service(ServeFile::new(index_file));
        let app = Router::new()
            .route("/ws", get(ws_handler))
            .fallback_service(serve_dir)
            .with_state(app_state);

        let addr = format!("{host}:{port}");
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[tunaSalon] web: {addr} 바인드 실패: {e}");
                return;
            }
        };
        eprintln!("[tunaSalon] web 서버: http://{addr} (LAN 접속 가능, /ws WebSocket)");
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("[tunaSalon] web serve 오류: {e}");
        }
        if let Some(engine_handle) = engine_handle {
            let _ = engine_handle.join();
        }
    });
}

// ── 직렬화 단위 테스트 ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_frame_serializes_with_required_keys() {
        let mut intensities = BTreeMap::new();
        intensities.insert("friend".to_string(), 0.72);
        intensities.insert("chaos".to_string(), 0.55);
        intensities.insert("summarizer".to_string(), 0.28);

        let participants = vec![
            Participant {
                id: "friend".to_string(),
                name: "Friendly Regular".to_string(),
                model: Some("gemma4:31b-cloud".to_string()),
                axes: None,
            },
            Participant {
                id: "나".to_string(),
                name: "나".to_string(),
                model: None,
                axes: None,
            },
        ];

        let frame = ServerFrame::State {
            room_id: "room1".to_string(),
            intensities,
            theta: 0.60,
            flow: 0.08,
            mu_scale: 1.0,
            liveliness: 0.4,
            pending: None,
            participants,
            messages: vec![],
            topics: vec!["부처님 오신날".to_string()],
            paused: false,
            tick_ms: 4000,
            reports: vec![],
        };

        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");

        assert_eq!(v["type"], "state");
        assert_eq!(v["room_id"], "room1", "room_id 키 필요");
        assert!(v["intensities"].is_object(), "intensities 키 필요");
        assert!(v["theta"].is_number(), "theta 키 필요");
        assert!(v["flow"].is_number(), "flow 키 필요");
        assert!(v["mu_scale"].is_number(), "mu_scale 키 필요");
        assert!(v["liveliness"].is_number(), "liveliness 키 필요");
        assert!(v["pending"].is_null(), "pending None → null 이어야 함");
        assert!(v["participants"].is_array(), "participants 키 필요");
        assert!(v["topics"].is_array(), "topics 키 필요");
        assert_eq!(v["paused"], false, "paused 키 필요");
        assert_eq!(v["tick_ms"], 4000u64, "tick_ms 키 필요");

        // intensities 값 검증
        assert!((v["intensities"]["friend"].as_f64().unwrap() - 0.72).abs() < 1e-9);
        // liveliness 값 검증
        assert!((v["liveliness"].as_f64().unwrap() - 0.4).abs() < 1e-9);
    }

    #[test]
    fn state_frame_pending_some_serializes_as_string() {
        let frame = ServerFrame::State {
            room_id: "room1".to_string(),
            intensities: BTreeMap::new(),
            theta: 0.6,
            flow: 0.0,
            mu_scale: 1.0,
            liveliness: 0.0,
            pending: Some("friend".to_string()),
            participants: vec![],
            messages: vec![],
            topics: vec![],
            paused: false,
            tick_ms: 4000,
            reports: vec![],
        };

        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");
        assert_eq!(v["pending"], "friend");
    }

    #[test]
    fn state_frame_paused_true_serializes() {
        let frame = ServerFrame::State {
            room_id: "room1".to_string(),
            intensities: BTreeMap::new(),
            theta: 0.6,
            flow: 0.0,
            mu_scale: 1.0,
            liveliness: 0.0,
            pending: None,
            participants: vec![],
            messages: vec![],
            topics: vec![],
            paused: true,
            tick_ms: 4000,
            reports: vec![],
        };

        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");
        assert_eq!(v["paused"], true, "paused true 직렬화");
    }

    #[test]
    fn client_pause_frame_deserializes() {
        let json = r#"{"type":"pause","paused":true}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Pause { paused } => assert!(paused),
            _ => panic!("ClientFrame::Pause 이어야 함"),
        }
    }

    #[test]
    fn client_pause_frame_false_deserializes() {
        let json = r#"{"type":"pause","paused":false}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Pause { paused } => assert!(!paused),
            _ => panic!("ClientFrame::Pause false 이어야 함"),
        }
    }

    #[test]
    fn utterance_frame_serializes_with_required_keys() {
        let frame = ServerFrame::Utterance {
            speaker: "chaos".to_string(),
            name: "Grounded Realist".to_string(),
            content: "현실적으로 생각해봐.".to_string(),
            ts: 173.0,
        };

        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");

        assert_eq!(v["type"], "utterance");
        assert_eq!(v["speaker"], "chaos");
        assert_eq!(v["name"], "Grounded Realist");
        assert_eq!(v["content"], "현실적으로 생각해봐.");
        assert!((v["ts"].as_f64().unwrap() - 173.0).abs() < 1e-9);
    }

    #[test]
    fn client_message_frame_deserializes() {
        let json = r#"{"type":"message","text":"hi"}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Message { text } => assert_eq!(text, "hi"),
            _ => panic!("ClientFrame::Message 이어야 함"),
        }
    }

    #[test]
    fn client_topic_frame_deserializes() {
        let json = r#"{"type":"topic","topics":["a","b"]}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Topic { topics } => {
                assert_eq!(topics, vec!["a", "b"]);
            }
            _ => panic!("ClientFrame::Topic 이어야 함"),
        }
    }

    #[test]
    fn client_invite_frame_deserializes() {
        let json = r#"{"type":"invite","blood":"O","mbti":"ENTJ","zodiac":"can"}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Invite {
                blood,
                mbti,
                zodiac,
                role,
            } => {
                assert_eq!(blood, "O");
                assert_eq!(mbti, "ENTJ");
                assert_eq!(zodiac, "can");
                assert_eq!(role, None, "role 생략 시 None이어야 함");
            }
            _ => panic!("ClientFrame::Invite 이어야 함"),
        }
    }

    #[test]
    fn client_invite_frame_with_role_deserializes() {
        let json = r#"{"type":"invite","blood":"A","mbti":"INFP","zodiac":"pis","role":"poet"}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Invite {
                blood,
                mbti,
                zodiac,
                role,
            } => {
                assert_eq!(blood, "A");
                assert_eq!(mbti, "INFP");
                assert_eq!(zodiac, "pis");
                assert_eq!(role, Some("poet".to_string()));
            }
            _ => panic!("ClientFrame::Invite 이어야 함"),
        }
    }

    #[test]
    fn client_remove_frame_deserializes() {
        let json = r#"{"type":"remove","id":"평화로운태양아래에서"}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Remove { id } => {
                assert_eq!(id, "평화로운태양아래에서");
            }
            _ => panic!("ClientFrame::Remove 이어야 함"),
        }
    }

    #[test]
    fn client_pace_frame_deserializes() {
        let json = r#"{"type":"pace","interval_ms":6000}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Pace { interval_ms } => assert_eq!(interval_ms, 6000),
            _ => panic!("ClientFrame::Pace 이어야 함"),
        }
    }

    #[test]
    fn client_presence_frame_deserializes() {
        let json = r#"{"type":"presence","clients":2}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Presence { clients } => assert_eq!(clients, 2),
            _ => panic!("ClientFrame::Presence 이어야 함"),
        }
    }

    #[test]
    fn client_reset_frame_deserializes() {
        let json = r#"{"type":"reset","topics":["AI 규제와 오픈소스"]}"#;
        let frame: ClientFrame = serde_json::from_str(json).expect("역직렬화 실패");
        match frame {
            ClientFrame::Reset { topics } => assert_eq!(topics, vec!["AI 규제와 오픈소스"]),
            _ => panic!("ClientFrame::Reset 이어야 함"),
        }
    }

    #[test]
    fn effective_paused_tracks_manual_presence_and_backend() {
        assert!(
            effective_paused(false, 0, false),
            "클라이언트 0명이면 자동 정지"
        );
        assert!(
            !effective_paused(false, 1, false),
            "접속자가 있고 수동/백엔드 정지가 없으면 진행"
        );
        assert!(
            effective_paused(true, 1, false),
            "수동 정지는 접속자가 있어도 유지"
        );
        assert!(
            effective_paused(false, 1, true),
            "백엔드 장애 정지는 접속자가 있어도 유지"
        );
    }

    #[test]
    fn state_frame_tick_ms_serializes() {
        let frame = ServerFrame::State {
            room_id: "room1".to_string(),
            intensities: BTreeMap::new(),
            theta: 0.6,
            flow: 0.0,
            mu_scale: 1.0,
            liveliness: 0.0,
            pending: None,
            participants: vec![],
            messages: vec![],
            topics: vec![],
            paused: false,
            tick_ms: 6000,
            reports: vec![],
        };
        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");
        assert_eq!(v["tick_ms"], 6000u64, "tick_ms 직렬화 확인");
    }

    #[test]
    fn web_startup_debate_normalizes_topics() {
        // 콤마는 주제 문장 안에 보존(쪼개지 않음). 여러 주제는 줄바꿈으로만 분리.
        // trim + 빈 항목 제거.
        let startup = WebStartup::debate(vec![
            "  AI regulation, open source ".to_string(),
            "".to_string(),
            "education\nethics".to_string(),
        ]);

        assert_eq!(
            startup.topics(),
            &[
                "AI regulation, open source".to_string(),
                "education".to_string(),
                "ethics".to_string()
            ]
        );
        assert!(startup
            .opening_prompt()
            .expect("opening prompt")
            .contains("AI regulation, open source"));
    }

    /// State 프레임에 liveliness 키가 number로 직렬화된다.
    #[test]
    fn state_frame_liveliness_serializes_as_number() {
        let frame = ServerFrame::State {
            room_id: "room1".to_string(),
            intensities: BTreeMap::new(),
            theta: 0.6,
            flow: 0.0,
            mu_scale: 1.0,
            liveliness: 0.75,
            pending: None,
            participants: vec![],
            messages: vec![],
            topics: vec![],
            paused: false,
            tick_ms: 6000,
            reports: vec![],
        };
        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");
        assert!(
            v["liveliness"].is_number(),
            "liveliness 키가 number이어야 함"
        );
        assert!((v["liveliness"].as_f64().unwrap() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn backend_to_model_maps_correctly() {
        assert_eq!(backend_to_model("cloud"), "gemma4:31b-cloud");
        assert_eq!(backend_to_model("friend"), "qwen3.6-35b-fast");
        assert_eq!(backend_to_model("custom"), "custom");
    }

    #[test]
    fn system_frame_serializes() {
        let frame = ServerFrame::System {
            text: "화제가 '부처님 오신날'로 바뀌었습니다".to_string(),
        };
        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");
        assert_eq!(v["type"], "system");
        assert_eq!(v["text"], "화제가 '부처님 오신날'로 바뀌었습니다");
    }

    /// Participant에 axes Some이 있으면 직렬화에 포함된다.
    #[test]
    fn participant_axes_some_serializes() {
        let p = Participant {
            id: "entp_o_leo_friend".to_string(),
            name: "호기심발랄레오".to_string(),
            model: Some("gemma4:31b-cloud".to_string()),
            axes: Some(ParticipantAxes {
                blood: "O".to_string(),
                mbti: "ENTP".to_string(),
                zodiac: "leo".to_string(),
                role: "friend".to_string(),
            }),
        };
        let json = serde_json::to_string(&p).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");
        assert_eq!(v["axes"]["blood"], "O");
        assert_eq!(v["axes"]["mbti"], "ENTP");
        assert_eq!(v["axes"]["zodiac"], "leo");
        assert_eq!(v["axes"]["role"], "friend");
    }

    /// Participant에 axes None이면 직렬화에서 axes 키가 누락된다(skip_serializing_if).
    #[test]
    fn participant_axes_none_omitted() {
        let p = Participant {
            id: "friend".to_string(),
            name: "데모친구".to_string(),
            model: None,
            axes: None,
        };
        let json = serde_json::to_string(&p).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");
        assert!(v.get("axes").is_none(), "axes None -> 키 없어야 함");
    }

    // ── build_state 단위 테스트 ─────────────────────────────────────────

    fn make_test_session(personas: Vec<crate::model::Persona>, human_id: &str) -> crate::live::LiveSession {
        use crate::pool::{BackendConfig, BackendPool};
        use std::sync::Arc;
        use std::time::Duration;
        let mut pool = BackendPool::new();
        pool.add(
            BackendConfig::new("offline", "m", "http://127.0.0.1:1", None, 1, None, Duration::from_millis(1)),
            std::collections::BTreeMap::new(),
        );
        pool.set_default("offline");
        let config = crate::model::EngineConfig {
            beta: 0.5,
            theta: 0.5,
            k: 60.0,
            tick_interval: 1.0,
            alpha: crate::model::CouplingMatrix::default(),
            forbid_self_repeat: false,
        };
        crate::live::LiveSession::new(config, personas, 42, Arc::new(pool), human_id)
    }

    /// build_state: "(진행)" 화자 이벤트는 messages 에서 제외된다.
    #[test]
    fn build_state_excludes_jinhaeng_speaker() {
        let mut session = make_test_session(
            vec![crate::model::Persona { id: "aria".to_string(), name: "Aria".to_string(), base_rate: 0.7 }],
            "you",
        );
        session.restore_history(
            vec![
                crate::model::Event { ts: 1.0, speaker: crate::live::MODERATOR_SPEAKER.to_string(), mark: 0.0, content: Some("사회자 멘트".to_string()) },
                crate::model::Event { ts: 2.0, speaker: "aria".to_string(), mark: 0.5, content: Some("안녕".to_string()) },
            ],
            2,
        );
        let frame = build_state(&session, "you", false, 6000, &[]);
        if let ServerFrame::State { messages, .. } = frame {
            assert_eq!(messages.len(), 1, "(진행) 메시지는 제외되어야 함");
            assert_eq!(messages[0].speaker, "aria");
        } else {
            panic!("State 프레임이어야 함");
        }
    }

    /// build_state: axes 없는 human speaker 의 name 이 human_id 그대로다.
    #[test]
    fn build_state_human_name_equals_human_id_without_axes() {
        let mut session = make_test_session(vec![], "나");
        session.restore_history(
            vec![crate::model::Event { ts: 1.0, speaker: "나".to_string(), mark: 1.0, content: Some("반가워요".to_string()) }],
            1,
        );
        let frame = build_state(&session, "나", false, 6000, &[]);
        if let ServerFrame::State { messages, .. } = frame {
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].name, "나", "human_id='나', axes 없음 → name == human_id");
        } else {
            panic!("State 프레임이어야 함");
        }
    }

    /// build_state: "토론을 시작합니다." 로 시작하는 human 발화는 messages 에서 제외된다.
    #[test]
    fn build_state_filters_toran_sijakhabnida() {
        let mut session = make_test_session(vec![], "you");
        session.restore_history(
            vec![
                crate::model::Event { ts: 1.0, speaker: "you".to_string(), mark: 1.0, content: Some("토론을 시작합니다. 주제는 AI입니다.".to_string()) },
                crate::model::Event { ts: 2.0, speaker: "you".to_string(), mark: 1.0, content: Some("안녕하세요".to_string()) },
            ],
            2,
        );
        let frame = build_state(&session, "you", false, 6000, &[]);
        if let ServerFrame::State { messages, .. } = frame {
            assert_eq!(messages.len(), 1, "토론 시작 메시지는 필터되어야 함");
            assert_eq!(messages[0].content, "안녕하세요");
        } else {
            panic!("State 프레임이어야 함");
        }
    }
}
