//! Debate producer layer (framework-independent).
//!
//! 토론을 "정중한 에세이 교환"에서 "재미있는 토론"으로 끌어올리는 *연출자* 로직.
//! 발화 지시(directive)·길이/형식 변주(format)·텍스트 정규화(text) 등 **순수 함수**만
//! 모은다.
//!
//! 경계 규칙(framework-independent core):
//! - 이 모듈은 `live`/`web`/`pool`/네트워크/tokio를 import하지 않는다. 입력은 plain
//!   데이터(topics, history 슬라이스, tick, speaker 등), 출력은 지시 텍스트와 선택 힌트뿐.
//! - 의존 방향은 단방향(`live → debate`). 역방향 금지.
//! - rng·IO·상태 없음 → 골든 불변식(LLM-off 바이트 동일)에 무영향. `LiveSession`은
//!   결과를 `history_snapshot`(복제본)에만 주입한다.
//!
//! Stage A(2026-06-26): `live.rs`에 흩어져 있던 producer 순수 로직을 행위 동일하게 이관.
//! 이후 DebatePlan/숨은목표/evidence card 등은 이 모듈에서 자란다(`docs/plans/salon-debate-producer.md`).

mod directive;
mod format;
pub mod plan;
mod text;

pub(crate) use directive::{
    build_directive, cross_room_memory_is_topic_relevant, repetition_guard,
    significant_topic_tokens,
};
pub(crate) use format::length_hint;
pub use plan::{infer_debate_plan, DebateMode, DebatePlan};
pub(crate) use text::{
    mentioned_persona_id, sanitize_generated_text, strip_speaker_prefix, summary_persona_id,
};
