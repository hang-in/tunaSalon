//! 토론 리포트 생성의 순수 부분: 프롬프트 빌드 + 결론 추출.
//!
//! 네트워크·LLM 호출 없음 - 순수 문자열 변환만. live.rs `summarize_debate`에서
//! 분리(R2 L4). 반환값은 live.rs 기존 코드와 byte 동일.

/// 메타 분석가 debrief 프롬프트를 빌드한다.
///
/// - `topic`: 토론 주제 문자열 (self.topics.join(", ") 결과).
/// - `past_conclusions`: 이전 라운드 결론 슬라이스. 비었으면 past_context 섹션 없음.
/// - `participants`: 실제 발언한 화자 명단(사람 '나' 포함 가능). 결론의 "참가자 입장"이
///   조용한 참가자/사람을 빠뜨리지 않도록 프롬프트에 전원 명시한다.
pub fn build_debrief_prompt(
    topic: &str,
    past_conclusions: &[String],
    participants: &[String],
) -> String {
    let past_context = if past_conclusions.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = past_conclusions
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}. {c}", i + 1))
            .collect();
        format!(
            "이전 토론 결론(맥락 참고용, 평가 대상 아님):\n{}\n\n",
            items.join("\n")
        )
    };
    // 참가자 입장 섹션 지시: 명단이 있으면 전원 각각 한 줄, 없으면 일반 지시.
    let stance_directive = if participants.is_empty() {
        "(각 참가자마다 `- **닉네임**: 핵심 주장` 형식 한 줄씩.)".to_string()
    } else {
        format!(
            "(참가자: {}. 위 참가자 전원 각각의 입장을 빠짐없이 `- **닉네임**: 핵심 주장` 형식으로 \
             한 줄씩 정리하라. 발언이 적었던 참가자나 사람('나')도 한 명도 빼지 말고 반드시 포함하라.)",
            participants.join(", ")
        )
    };
    format!(
        "{past_context}You are a neutral debate analyst. The discussion above is a FINISHED debate on the topic \"{topic}\". \
         Write a DEBRIEF REPORT in Korean using GitHub-flavored MARKDOWN - this is a report document, NOT a chat reply, \
         so do not address anyone or continue the debate. Lead with the conclusion (두괄식): the report MUST start with the \
         '## 결론' section. Use exactly these sections in this order:\n\
         ## 결론\n\
         (2-3 문장: 한 줄 핵심 결론 먼저 - 무엇으로 귀결됐는지 또는 끝내 갈렸는지, 가장 설득력 있던 논지.)\n\
         ## 주제\n\
         (한 줄.)\n\
         ## 참가자 입장\n\
         {stance_directive}\n\
         ## 합의점\n\
         (동의한 지점. 없으면 '뚜렷한 합의 없음'.)\n\
         ## 끝까지 갈린 지점\n\
         (합의되지 않은 핵심 쟁점.)\n\
         Stay objective, do not take a side, do not invent new arguments. Use markdown headings, bold, and bullet lists. Korean only.",
        topic = topic,
        stance_directive = stance_directive
    )
}

/// '## 결론' 섹션 본문(다음 `##` 전까지)을 공백으로 이어 반환한다.
/// 섹션이 없으면 첫 줄 반환.
pub fn extract_conclusion_section(markdown: &str) -> String {
    let mut in_section = false;
    let mut body = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("## 결론") {
            in_section = true;
            continue;
        }
        if in_section {
            if trimmed.starts_with("##") {
                break;
            }
            if !trimmed.is_empty() {
                body.push(trimmed.to_string());
            }
        }
    }
    if body.is_empty() {
        markdown.lines().next().unwrap_or("").to_string()
    } else {
        body.join(" ")
    }
}
