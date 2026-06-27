//! web 프런트엔드 wire DTO/protocol 타입 (serde 직렬화 전용).
//!
//! web.rs(god-file)에서 분리한 데이터 계약 계층. derive·serde 속성 무변경이라
//! 직렬화 형식은 기존과 byte 동일. 전부 web 모듈 내부 전용(`pub(super)`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Participant의 4축 정보 (직렬화 전용).
#[derive(Serialize, Clone)]
pub(super) struct ParticipantAxes {
    pub(super) blood: String,
    pub(super) mbti: String,
    pub(super) zodiac: String,
    pub(super) role: String,
}

#[derive(Serialize, Clone)]
pub(super) struct Participant {
    pub(super) id: String,
    pub(super) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) axes: Option<ParticipantAxes>,
}

#[derive(Serialize, Clone)]
pub(super) struct HistoryMessage {
    pub(super) speaker: String,
    pub(super) name: String,
    pub(super) content: String,
    pub(super) ts: f64,
}

/// 클라이언트에 전달하는 리포트 DTO.
#[derive(Serialize, Clone)]
pub(super) struct ReportDto {
    pub(super) seq: u32,
    pub(super) created_at: i64,
    pub(super) topic: String,
    pub(super) markdown: String,
    pub(super) conclusion: String,
}

impl From<crate::roomstore::ReportRecord> for ReportDto {
    fn from(r: crate::roomstore::ReportRecord) -> Self {
        Self {
            seq: r.seq,
            created_at: r.created_at,
            topic: r.topic,
            markdown: r.markdown,
            conclusion: r.conclusion,
        }
    }
}

/// GET /api/rooms/{room_id}/report 응답 DTO.
#[derive(Serialize)]
pub(super) struct RoomReportResponse {
    pub(super) concluded: bool,
    pub(super) summary: String,
}

#[derive(Serialize)]
#[serde(tag = "type")]
pub(super) enum ServerFrame {
    #[serde(rename = "state")]
    State {
        room_id: String,
        intensities: BTreeMap<String, f64>,
        theta: f64,
        flow: f64,
        mu_scale: f64,
        liveliness: f64,
        pending: Option<String>,
        participants: Vec<Participant>,
        messages: Vec<HistoryMessage>,
        topics: Vec<String>,
        paused: bool,
        tick_ms: u64,
        reports: Vec<ReportDto>,
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
    #[serde(rename = "report")]
    Report { text: String },
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub(super) enum ClientFrame {
    #[serde(rename = "message")]
    Message { text: String },
    #[serde(rename = "topic")]
    Topic { topics: Vec<String> },
    #[serde(rename = "pause")]
    Pause { paused: bool },
    #[serde(rename = "pace")]
    Pace { interval_ms: u64 },
    #[serde(rename = "invite")]
    Invite {
        blood: String,
        mbti: String,
        zodiac: String,
        #[serde(default)]
        role: Option<String>,
    },
    #[serde(rename = "remove")]
    Remove { id: String },
    #[serde(rename = "presence")]
    Presence { clients: usize },
    #[serde(rename = "reset")]
    Reset { topics: Vec<String> },
    #[serde(rename = "human_profile")]
    HumanProfile {
        blood: String,
        mbti: String,
        zodiac: String,
        role: String,
    },
}
