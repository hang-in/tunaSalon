#![cfg(feature = "web")]
//! web 프런트엔드 sink: axum WebSocket으로 엔진 이벤트를 브라우저에 push + 사람 입력 수신.
//! 엔진은 blocking(전용 스레드), axum은 tokio(async). 둘은 tokio 채널로 브리지.

use crate::live::LiveSession;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

// ── 프레임 스키마 ──────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct Participant {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "type")]
enum ServerFrame {
    #[serde(rename = "state")]
    State {
        intensities: BTreeMap<String, f64>,
        theta: f64,
        flow: f64,
        mu_scale: f64,
        pending: Option<String>,
        participants: Vec<Participant>,
        topics: Vec<String>,
    },
    #[serde(rename = "utterance")]
    Utterance {
        speaker: String,
        name: String,
        content: String,
        ts: f64,
    },
    #[serde(rename = "system")]
    System { text: String },
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ClientFrame {
    #[serde(rename = "message")]
    Message { text: String },
    #[serde(rename = "topic")]
    Topic { topics: Vec<String> },
}

#[allow(dead_code)]
enum EngineCmd {
    Human(String),
    Topic(Vec<String>),
    Shutdown,
}

const STATE_PERIOD: Duration = Duration::from_millis(700);
const TICK_PERIOD: Duration = Duration::from_millis(2000);
const POLL_PERIOD: Duration = Duration::from_millis(80);

// 엔진 스레드: blocking LiveSession 구동, frame을 broadcast로 push, cmd를 mpsc로 수신.
fn run_engine(
    mut session: LiveSession,
    human_id: String,
    models: BTreeMap<String, String>,
    frame_tx: broadcast::Sender<String>,
    mut cmd_rx: mpsc::UnboundedReceiver<EngineCmd>,
) {
    // personas(id, name) 스냅샷(participants 빌드용)
    let persona_meta: Vec<(String, String)> = session
        .personas()
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();

    let emit = |tx: &broadcast::Sender<String>, frame: &ServerFrame| {
        if let Ok(json) = serde_json::to_string(frame) {
            let _ = tx.send(json); // 구독자 없어도 무시(broadcast)
        }
    };

    let build_state = |session: &LiveSession| -> ServerFrame {
        let intensities: BTreeMap<String, f64> =
            session.combined_intensities().into_iter().collect();
        let mut participants: Vec<Participant> = persona_meta
            .iter()
            .map(|(id, name)| Participant {
                id: id.clone(),
                name: name.clone(),
                model: models.get(id).cloned(),
            })
            .collect();
        participants.push(Participant {
            id: human_id.clone(),
            name: human_id.clone(),
            model: None,
        });
        ServerFrame::State {
            intensities,
            theta: session.theta(),
            flow: session.flow().map(|f| f.convergence).unwrap_or(0.0),
            mu_scale: session.mu_scale(),
            pending: session.pending_speaker(),
            participants,
            topics: session.topics().to_vec(),
        }
    };

    let mut last_state = Instant::now();
    let mut last_tick = Instant::now();
    // 초기 state 1회
    emit(&frame_tx, &build_state(&session));

    loop {
        // 1. 명령 처리(즉시 반응)
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                EngineCmd::Human(text) => {
                    session.submit_human(text.clone());
                    let ts = session
                        .state()
                        .history
                        .last()
                        .map(|e| e.ts)
                        .unwrap_or(0.0);
                    emit(
                        &frame_tx,
                        &ServerFrame::Utterance {
                            speaker: human_id.clone(),
                            name: human_id.clone(),
                            content: text,
                            ts,
                        },
                    );
                    emit(&frame_tx, &build_state(&session)); // λ 스파이크 즉시 반영
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
                    emit(&frame_tx, &build_state(&session));
                }
                EngineCmd::Shutdown => {
                    session.shutdown();
                    return;
                }
            }
        }

        // 2. tick (주기)
        if last_tick.elapsed() >= TICK_PERIOD {
            session.tick();
            last_tick = Instant::now();
        }

        // 3. 완성 발화 drain -> utterance frame
        while let Some(ev) = session.poll_generation() {
            if let Some(content) = ev.content {
                let name = persona_meta
                    .iter()
                    .find(|(id, _)| *id == ev.speaker)
                    .map(|(_, n)| n.clone())
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
            }
        }

        // 4. state frame (주기)
        if last_state.elapsed() >= STATE_PERIOD {
            emit(&frame_tx, &build_state(&session));
            last_state = Instant::now();
        }

        std::thread::sleep(POLL_PERIOD);
    }
}

// axum 라우터 + serve. main에서 호출(blocking, 내부에서 tokio runtime).
pub fn serve(host: &str, port: u16, session: LiveSession, human_id: String, models: BTreeMap<String, String>) {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[tunaSalon] web: tokio runtime 생성 실패: {e}");
            return;
        }
    };
    rt.block_on(async move {
        let (frame_tx, _) = broadcast::channel::<String>(256);
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<EngineCmd>();

        // 엔진 전용 스레드(blocking)
        let frame_tx_engine = frame_tx.clone();
        let engine_handle = std::thread::spawn(move || {
            run_engine(session, human_id, models, frame_tx_engine, cmd_rx);
        });

        use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
        use axum::extract::State as AxumState;
        use axum::response::IntoResponse;
        use axum::{routing::get, Router};
        use tower_http::services::{ServeDir, ServeFile};

        #[derive(Clone)]
        struct AppState {
            frame_tx: broadcast::Sender<String>,
            cmd_tx: mpsc::UnboundedSender<EngineCmd>,
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
                        let cmd = match frame {
                            ClientFrame::Message { text } => EngineCmd::Human(text),
                            ClientFrame::Topic { topics } => EngineCmd::Topic(topics),
                        };
                        let _ = cmd_tx.send(cmd);
                    }
                }
            }
            send_task.abort();
        }

        let app_state = AppState {
            frame_tx: frame_tx.clone(),
            cmd_tx: cmd_tx.clone(),
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
        let _ = engine_handle.join();
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
            },
            Participant {
                id: "나".to_string(),
                name: "나".to_string(),
                model: None,
            },
        ];

        let frame = ServerFrame::State {
            intensities,
            theta: 0.60,
            flow: 0.08,
            mu_scale: 1.0,
            pending: None,
            participants,
            topics: vec!["부처님 오신날".to_string()],
        };

        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");

        assert_eq!(v["type"], "state");
        assert!(v["intensities"].is_object(), "intensities 키 필요");
        assert!(v["theta"].is_number(), "theta 키 필요");
        assert!(v["flow"].is_number(), "flow 키 필요");
        assert!(v["mu_scale"].is_number(), "mu_scale 키 필요");
        assert!(v["pending"].is_null(), "pending None → null 이어야 함");
        assert!(v["participants"].is_array(), "participants 키 필요");
        assert!(v["topics"].is_array(), "topics 키 필요");

        // intensities 값 검증
        assert!((v["intensities"]["friend"].as_f64().unwrap() - 0.72).abs() < 1e-9);
    }

    #[test]
    fn state_frame_pending_some_serializes_as_string() {
        let frame = ServerFrame::State {
            intensities: BTreeMap::new(),
            theta: 0.6,
            flow: 0.0,
            mu_scale: 1.0,
            pending: Some("friend".to_string()),
            participants: vec![],
            topics: vec![],
        };

        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");
        assert_eq!(v["pending"], "friend");
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
    fn system_frame_serializes() {
        let frame = ServerFrame::System {
            text: "화제가 '부처님 오신날'로 바뀌었습니다".to_string(),
        };
        let json = serde_json::to_string(&frame).expect("직렬화 실패");
        let v: serde_json::Value = serde_json::from_str(&json).expect("파싱 실패");
        assert_eq!(v["type"], "system");
        assert_eq!(v["text"], "화제가 '부처님 오신날'로 바뀌었습니다");
    }
}
