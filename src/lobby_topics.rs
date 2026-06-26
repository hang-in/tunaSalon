//! 로비 추천 토론 주제 생성기(web 전용).
//!
//! Ollama Web Search API(`ollama.com/api/web_search`)로 분야별 최신 헤드라인을 모으고,
//! cloud gemma(`localhost:11434/api/generate`)로 찬반이 갈리는 토론 주제 2~3개씩을 뽑는다.
//! 자체 HTTP 호출만 쓰므로 BackendPool 배선에 의존하지 않는다. 실패하면 None → 호출측이
//! 정적 폴백을 쓴다. golden/결정성과 무관(web feature + 네트워크 경로).

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 분야별 추천 주제. 프런트 `/api/suggested-topics` 응답 단위.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryTopics {
    pub category: String,
    pub topics: Vec<String>,
}

/// 검색할 분야(라벨, 검색 쿼리).
const CATEGORIES: [(&str, &str); 5] = [
    ("기술·AI", "AI 인공지능 윤리 규제 최신 논쟁 이슈"),
    ("사회·제도", "한국 사회 제도 정책 논쟁 이슈 최신"),
    ("윤리·가치", "윤리 가치관 도덕 딜레마 논쟁 최신"),
    ("관계·일상", "연애 가족 세대 관계 갈등 논쟁 최신"),
    ("미래·노동", "미래 노동 자동화 일자리 기본소득 논쟁"),
];

/// 웹서치/생성에 쓸 API 키를 고른다. `OLLAMA_API_KEY` → `OLLAMA_CLOUD_API_KEY` 순.
/// `ssh-`로 시작하는 값(= signin 공개키, Bearer 키 아님)은 건너뛴다.
fn resolve_api_key() -> Option<String> {
    for var in ["OLLAMA_API_KEY", "OLLAMA_CLOUD_API_KEY"] {
        if let Ok(v) = std::env::var(var) {
            let v = v.trim().to_string();
            if !v.is_empty() && !v.starts_with("ssh-") {
                return Some(v);
            }
        }
    }
    None
}

/// Ollama Web Search: 쿼리에 대한 최신 결과 제목들을 반환한다.
fn web_search(query: &str, key: &str) -> Option<Vec<String>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .ok()?;
    let resp = client
        .post("https://ollama.com/api/web_search")
        .header("Authorization", format!("Bearer {key}"))
        .json(&serde_json::json!({ "query": query, "max_results": 4 }))
        .send()
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().ok()?;
    let results = v.get("results")?.as_array()?;
    let titles: Vec<String> = results
        .iter()
        .filter_map(|r| r.get("title").and_then(|t| t.as_str()).map(|s| s.to_string()))
        .collect();
    if titles.is_empty() {
        None
    } else {
        Some(titles)
    }
}

/// cloud gemma에 평문 프롬프트를 보내 응답 텍스트를 받는다(로컬 데몬 프록시 경유).
fn gemma_generate(prompt: &str) -> Option<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .ok()?;
    let resp = client
        .post("http://localhost:11434/api/generate")
        .json(&serde_json::json!({
            "model": "gemma4:31b-cloud",
            "prompt": prompt,
            "stream": false
        }))
        .send()
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: serde_json::Value = resp.json().ok()?;
    v.get("response").and_then(|r| r.as_str()).map(|s| s.to_string())
}

/// gemma 응답에서 JSON 배열만 추출해 파싱한다(앞뒤 잡텍스트/코드펜스 허용).
fn parse_topics_json(raw: &str) -> Option<Vec<CategoryTopics>> {
    let start = raw.find('[')?;
    let end = raw.rfind(']')?;
    if end <= start {
        return None;
    }
    let parsed: Vec<CategoryTopics> = serde_json::from_str(&raw[start..=end]).ok()?;
    let cleaned: Vec<CategoryTopics> = parsed
        .into_iter()
        .filter_map(|c| {
            let category = c.category.trim().to_string();
            if category.is_empty() {
                return None;
            }
            let topics: Vec<String> = c
                .topics
                .into_iter()
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .take(3)
                .collect();
            if topics.is_empty() {
                None
            } else {
                Some(CategoryTopics { category, topics })
            }
        })
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// 분야별 추천 토론 주제를 생성한다(블로킹 — 백그라운드/spawn_blocking에서 호출).
/// 키가 없거나 검색/생성/파싱이 실패하면 None.
pub fn generate_suggested_topics() -> Option<Vec<CategoryTopics>> {
    let key = resolve_api_key()?;
    let mut blocks = Vec::new();
    for (cat, query) in CATEGORIES {
        if let Some(titles) = web_search(query, &key) {
            blocks.push(format!("[{cat}] 최근 헤드라인:\n- {}", titles.join("\n- ")));
        }
    }
    if blocks.is_empty() {
        return None;
    }
    let prompt = format!(
        "다음은 분야별 최근 뉴스 헤드라인이다. 각 분야마다 찬반·조건이 갈리는 흥미로운 토론 주제를 \
         정확히 2~3개씩 만들어라. 각 주제는 한국어 한 문장 질문형으로 \
         (예: \"AI 판사가 인간 판사보다 공정할 수 있을까?\"). 정치 선동·특정 인물 비방·단순 사실확인은 \
         피하고, 가치가 충돌하는 주제로. \
         추가로 검색 결과와 무관하게 \"재미·취향\" 분야를 하나 더 만들어, 진지한 정책이 아니라 가볍고 \
         호불호가 분명히 갈리는 일상 논쟁 주제 2~3개를 넣어라(예: 음식 취향, 생활 습관, 사소한 선택 등 \
         유쾌하게 다툴 만한 것). 반드시 아래 JSON 배열만 출력하고 다른 텍스트는 쓰지 마라:\n\
         [{{\"category\":\"분야명\",\"topics\":[\"주제1\",\"주제2\"]}}]\n\n{}",
        blocks.join("\n\n")
    );
    let raw = gemma_generate(&prompt)?;
    parse_topics_json(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_topics_json_extracts_array_with_surrounding_text() {
        let raw = "여기 있습니다:\n```json\n[{\"category\":\"기술\",\"topics\":[\"A?\",\"B?\",\"C?\",\"D?\"]}]\n```";
        let got = parse_topics_json(raw).expect("should parse");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].category, "기술");
        assert_eq!(got[0].topics.len(), 3, "topics capped at 3");
    }

    #[test]
    fn parse_topics_json_drops_empty_and_invalid() {
        assert!(parse_topics_json("no json here").is_none());
        assert!(parse_topics_json("[{\"category\":\"\",\"topics\":[\"x\"]}]").is_none());
        assert!(parse_topics_json("[{\"category\":\"c\",\"topics\":[]}]").is_none());
    }

    #[test]
    fn resolve_api_key_skips_ssh_value() {
        std::env::set_var("OLLAMA_API_KEY", "ssh-ed25519 AAAAfake");
        std::env::set_var("OLLAMA_CLOUD_API_KEY", "abc.def");
        assert_eq!(resolve_api_key().as_deref(), Some("abc.def"));
        std::env::remove_var("OLLAMA_API_KEY");
        std::env::remove_var("OLLAMA_CLOUD_API_KEY");
    }
}
