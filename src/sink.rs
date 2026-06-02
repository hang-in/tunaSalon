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
}
