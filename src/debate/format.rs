//! 발화 형식/길이 변주 힌트(순수). 향후 DebatePlan 기반 format 변주가 이 자리에서 자란다.

/// 발화 길이 변주 힌트(생성 워커 프롬프트용).
///
/// tick + 화자 기반 결정적 선택이라 **rng를 소비하지 않는다**(골든·화자선택 결정성 무영향).
/// history_snapshot(복제본)에만 주입되어 state.history는 불변(INV-2). 라이브 발화 길이를
/// 일률적이지 않게 흩뜨리는 용도.
pub(crate) fn length_hint(tick: u64, speaker: &str) -> &'static str {
    let salt: usize = speaker.bytes().map(|b| b as usize).sum();
    match (tick as usize).wrapping_add(salt) % 4 {
        0 => "[길이] 3-4문장으로 답하세요. 주장, 근거, 상대 발화와의 연결을 포함하세요.",
        1 => "[길이] 4-5문장으로 답하세요. 찬반 입장을 분명히 하고 반례나 조건을 하나 넣으세요.",
        2 => "[길이] 5-6문장으로 조금 길게 답하세요. 상대 닉네임을 부르며 핵심 전제를 짚으세요.",
        _ => "[길이] 3-5문장으로 답하세요. 짧은 감상 대신 토론 가능한 주장으로 말하세요.",
    }
}
