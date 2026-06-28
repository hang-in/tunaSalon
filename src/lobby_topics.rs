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

/// 검색할 분야(라벨, 쿼리 풀). cycle마다 풀에서 다른 쿼리를 골라 헤드라인이 정체되지 않게 한다.
const CATEGORIES: [(&str, &[&str]); 5] = [
    (
        "기술·AI",
        &[
            "AI 인공지능 윤리 규제 최신 논쟁 이슈",
            "생성형 AI 저작권 창작 일자리 논란 최신",
            "AI 감시 프라이버시 데이터 활용 논쟁 최신",
            "자율주행 로봇 자동화 안전 책임 논쟁 최신",
        ],
    ),
    (
        "사회·제도",
        &[
            "한국 사회 제도 정책 논쟁 이슈 최신",
            "교육 입시 경쟁 공정성 논쟁 최신",
            "주거 부동산 청년 정책 논쟁 최신",
            "복지 세금 형평성 논쟁 최신",
        ],
    ),
    (
        "윤리·가치",
        &[
            "윤리 가치관 도덕 딜레마 논쟁 최신",
            "표현의 자유 혐오 규제 논쟁 최신",
            "생명윤리 의료 선택 논쟁 최신",
            "환경 동물권 소비 윤리 논쟁 최신",
        ],
    ),
    (
        "관계·일상",
        &[
            "연애 가족 세대 관계 갈등 논쟁 최신",
            "결혼 비혼 출산 선택 논쟁 최신",
            "직장 예절 워라밸 세대차 논쟁 최신",
            "친구 돈 거리두기 관계 논쟁 최신",
        ],
    ),
    (
        "미래·노동",
        &[
            "미래 노동 자동화 일자리 기본소득 논쟁",
            "주4일제 재택근무 생산성 논쟁 최신",
            "긱이코노미 플랫폼 노동 논쟁 최신",
            "정년 연장 청년 고용 논쟁 최신",
        ],
    ),
];

/// "재미·취향" 분야가 매번 같은 주제(민초/부먹)로 굳지 않게 cycle마다 도메인을 바꾼다.
const CASUAL_ANGLES: [&str; 6] = [
    "음식 취향과 맛(민초·부먹찍먹·라면 끓이기 같은)",
    "생활 습관과 집안 환경(에어컨·정리정돈·아침형/저녁형 같은)",
    "디지털·SNS 예절과 습관(읽씹·단톡방·맞춤법 같은)",
    "연애·관계의 사소한 규칙(데이트 비용·연락 빈도·기념일 같은)",
    "여행·여가 취향(계획형/즉흥형·국내/해외·집순이/밖순이 같은)",
    "사소한 일상 선택(탕수육 소스·치약 짜는 법·물 온도 같은)",
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
            "stream": false,
            // 높은 temperature로 같은 헤드라인에서도 매번 다른 주제가 나오게(추천 정체 방지).
            "options": { "temperature": 1.0, "top_p": 0.95 }
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

/// 토픽 비교용 정규화: 영숫자/한글만 남기고 소문자화(공백·문장부호·물음표 무시).
fn norm_topic(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

/// 생성된 주제 중 최근 추천(avoid)과 정규화 일치하는 것을 제거한다(정확 중복 안전망).
/// 비슷한 변형은 프롬프트의 회피 지시가 1차로 거른다.
fn filter_recent(groups: Vec<CategoryTopics>, avoid: &[String]) -> Vec<CategoryTopics> {
    let recent_norm: std::collections::HashSet<String> =
        avoid.iter().map(|s| norm_topic(s)).collect();
    groups
        .into_iter()
        .map(|mut c| {
            c.topics.retain(|t| !recent_norm.contains(&norm_topic(t)));
            c
        })
        .filter(|c| !c.topics.is_empty())
        .collect()
}

/// 분야별 추천 토론 주제를 생성한다(블로킹 — 백그라운드/spawn_blocking에서 호출).
/// 키가 없거나 검색/생성/파싱이 실패하면 None.
///
/// `cycle`은 갱신 회차(쿼리 풀·앵글 회전). `avoid`는 최근 이미 추천한 주제로,
/// 프롬프트에 "다시 만들지 마라"로 주입 + 출력에서 정규화 중복 제거 → 같은 주제 반복 방지.
pub fn generate_suggested_topics(cycle: u64, avoid: &[String]) -> Option<Vec<CategoryTopics>> {
    let key = resolve_api_key()?;
    let mut blocks = Vec::new();
    for (cat, queries) in CATEGORIES {
        let query = queries[(cycle as usize) % queries.len()];
        if let Some(titles) = web_search(query, &key) {
            blocks.push(format!("[{cat}] 최근 헤드라인:\n- {}", titles.join("\n- ")));
        }
    }
    if blocks.is_empty() {
        return None;
    }
    let casual_angle = CASUAL_ANGLES[(cycle as usize) % CASUAL_ANGLES.len()];
    // 최근 추천한 주제(최대 30개)를 프롬프트에 명시해 재생성을 막는다.
    let avoid_block = if avoid.is_empty() {
        String::new()
    } else {
        let recent: Vec<&str> = avoid.iter().rev().take(30).map(|s| s.as_str()).collect();
        format!(
            "\n\n# 최근에 이미 추천한 주제다. 이 주제들과 같거나 살짝 바꾼 변형은 절대 만들지 마라(완전히 다른 주제로):\n- {}",
            recent.join("\n- ")
        )
    };
    let prompt = format!(
        "다음은 분야별 최근 뉴스 헤드라인이다. 각 분야마다 찬반·조건이 갈리는 흥미로운 토론 주제를 \
         정확히 2~3개씩 만들어라. 각 주제는 한국어 한 문장 질문형으로 \
         (예: \"AI 판사가 인간 판사보다 공정할 수 있을까?\"). 정치 선동·특정 인물 비방·단순 사실확인은 \
         피하고, 가치가 충돌하는 주제로. 진부하거나 자주 나오는 뻔한 주제는 피하고 새로운 각도로. \
         추가로 검색 결과와 무관하게 \"재미·취향\" 분야를 하나 더 만들어, 진지한 정책이 아니라 가볍고 \
         호불호가 분명히 갈리는 일상 논쟁 주제 2~3개를 넣어라. 이번엔 특히 '{casual_angle}' 쪽으로, \
         유쾌하게 다툴 만한 것으로(부먹/찍먹·민초 같은 흔한 건 피하라). \
         반드시 아래 JSON 배열만 출력하고 다른 텍스트는 쓰지 마라:\n\
         [{{\"category\":\"분야명\",\"topics\":[\"주제1\",\"주제2\"]}}]{avoid_block}\n\n{}",
        blocks.join("\n\n")
    );
    let raw = gemma_generate(&prompt)?;
    let parsed = parse_topics_json(&raw)?;
    let filtered = filter_recent(parsed, avoid);
    if filtered.is_empty() {
        None
    } else {
        Some(filtered)
    }
}

/// 최근 추천 주제 영속 파일 경로. `$SALON_TOPICS_HISTORY` → `$HOME/.local/share/tunaSalon/recent_topics.json`.
fn recent_topics_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("SALON_TOPICS_HISTORY") {
        if !p.trim().is_empty() {
            return Some(std::path::PathBuf::from(p));
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(std::path::PathBuf::from(home).join(".local/share/tunaSalon/recent_topics.json"))
}

/// 최근 추천 주제를 로드한다(재시작에도 중복 회피가 살아남게). 없으면 빈 Vec.
pub fn load_recent() -> Vec<String> {
    recent_topics_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

/// 최근 추천 주제를 저장한다(비치명: 실패해도 무시).
pub fn save_recent(recent: &[String]) {
    if let Some(p) = recent_topics_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(recent) {
            let _ = std::fs::write(p, json);
        }
    }
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
    fn filter_recent_drops_normalized_duplicates() {
        let groups = vec![CategoryTopics {
            category: "재미·취향".to_string(),
            topics: vec![
                "부먹 vs 찍먹, 진리는?".to_string(), // avoid에 정규화 일치 → 제거
                "민초는 음식인가?".to_string(),       // 유지
                "탕수육 소스 논쟁은 끝났을까?".to_string(), // 유지(신규)
            ],
        }];
        // 공백·문장부호가 달라도 정규화로 같은 주제는 제거되어야 한다.
        let avoid = vec!["부먹vs찍먹 진리는".to_string()];
        let out = filter_recent(groups, &avoid);
        assert_eq!(out.len(), 1);
        let topics = &out[0].topics;
        assert_eq!(topics.len(), 2, "부먹/찍먹 중복 1개 제거");
        assert!(!topics.iter().any(|t| t.contains("부먹")));
        assert!(topics.iter().any(|t| t.contains("탕수육")));
    }

    #[test]
    fn filter_recent_drops_category_when_all_duplicate() {
        let groups = vec![CategoryTopics {
            category: "재미·취향".to_string(),
            topics: vec!["민초는 음식인가?".to_string()],
        }];
        let avoid = vec!["민초는 음식인가".to_string()];
        assert!(filter_recent(groups, &avoid).is_empty(), "전부 중복이면 빈 결과");
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
