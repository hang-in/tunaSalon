use crate::model::PersonaId;
use serde::Serialize;
// 결정성: NDJSON 직렬화 시 키 순서가 실행마다 동일해야 하므로 BTreeMap(정렬 순서) 사용.
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ObservationRecord {
    pub tick: u64,
    pub ts: f64,
    pub intensities: BTreeMap<PersonaId, f64>,
    pub gate_passed: bool,
    pub candidates: Vec<PersonaId>,
    pub chosen: Option<PersonaId>,
    pub rrf_reason: Option<String>,
    pub silence_count: u64,
    pub speak_count: u64,
    pub conversation_len: u64,
    /// α=0이면 항목 없음 → 직렬화에서 생략(v0.1 골든 바이트 동일 보존).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub excitations: BTreeMap<PersonaId, f64>,
    /// FakeBackend이면 None → 직렬화에서 생략(v0.2 골든 바이트 동일 보존).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utterance: Option<String>,
    /// content 있는 발화 2개 미만이면 None → 직렬화에서 생략(FakeBackend 골든 바이트 동일 보존).
    /// 관찰 전용 — 엔진 선택/강도/파라미터에 영향 없음(INV-2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<crate::flow::FlowMetric>,
}

pub trait ObservationSink {
    fn emit(&mut self, record: &ObservationRecord);

    fn finish(&mut self) {}
}

#[derive(Debug, Default)]
pub struct VecSink {
    pub records: Vec<ObservationRecord>,
}

impl ObservationSink for VecSink {
    fn emit(&mut self, record: &ObservationRecord) {
        self.records.push(record.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> ObservationRecord {
        let mut intensities = BTreeMap::new();
        intensities.insert("p1".to_string(), 0.8);

        ObservationRecord {
            tick: 2,
            ts: 1.0,
            intensities,
            gate_passed: true,
            candidates: vec!["p1".to_string()],
            chosen: Some("p1".to_string()),
            rrf_reason: Some("intensity".to_string()),
            silence_count: 0,
            speak_count: 1,
            conversation_len: 1,
            excitations: BTreeMap::new(),
            utterance: None,
            flow: None,
        }
    }

    #[test]
    fn serializes_observation_record_as_single_line_json() {
        let record = sample_record();
        let json = serde_json::to_string(&record);

        assert!(json.is_ok());
        if let Ok(json) = json {
            assert!(!json.contains('\n'));
            assert!(json.starts_with('{'));
        }
    }

    #[test]
    fn vec_sink_emit_collects_records() {
        let record = sample_record();
        let mut sink = VecSink::default();

        sink.emit(&record);

        assert_eq!(sink.records.len(), 1);
        assert_eq!(sink.records[0], record);
    }

    /// (task-34) flow=None인 record는 JSON에 "flow" 키가 없어야 한다.
    /// skip_serializing_if = "Option::is_none" 동작 검증.
    #[test]
    fn record_with_flow_none_omits_flow_key_in_json() {
        let record = sample_record(); // flow: None
        let json = serde_json::to_string(&record).expect("직렬화 성공");
        assert!(
            !json.contains("\"flow\""),
            "flow=None이면 JSON에 \"flow\" 키가 없어야 한다. 실제: {json}"
        );
    }

    /// (task-34) flow=Some인 record는 JSON에 "flow" 키가 있어야 한다.
    #[test]
    fn record_with_flow_some_includes_flow_key_in_json() {
        let mut record = sample_record();
        record.flow = Some(crate::flow::FlowMetric { convergence: 0.42 });
        let json = serde_json::to_string(&record).expect("직렬화 성공");
        assert!(
            json.contains("\"flow\""),
            "flow=Some이면 JSON에 \"flow\" 키가 있어야 한다. 실제: {json}"
        );
        assert!(
            json.contains("\"convergence\""),
            "flow 값에 convergence 필드가 있어야 한다. 실제: {json}"
        );
    }
}
