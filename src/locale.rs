//! 시스템 로케일 → 응답 언어 감지 (v0.5 사람 참여 채팅방).
//!
//! 페르소나가 **사용자 시스템 언어**로 답하도록 프롬프트에 주입할 언어 이름을 정한다.
//! 영어 프롬프트만 주면 콜드스타트에 영어로 새므로, 명시적으로 언어를 지시한다.

/// 페르소나 응답 언어(영어 명칭, LLM 지시문용)를 반환한다.
///
/// 우선순위: `SALON_LANG`(명시 override) > `LC_ALL` > `LANG` > 기본(Korean).
/// 로케일 문자열(`ko_KR.UTF-8` 등)의 언어 부분만 보고 매핑한다.
pub fn reply_language() -> &'static str {
    let raw = std::env::var("SALON_LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default();
    language_name(&raw)
}

/// 로케일 문자열의 언어 코드를 LLM이 이해하는 영어 언어 명칭으로 매핑한다.
/// 미지원/빈 값은 기본 한국어(사용자 기본 언어).
fn language_name(locale: &str) -> &'static str {
    let code = locale
        .split(|c| c == '_' || c == '.' || c == '-')
        .next()
        .unwrap_or("")
        .to_lowercase();
    match code.as_str() {
        "ko" => "Korean",
        "en" => "English",
        "ja" => "Japanese",
        "zh" => "Chinese",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "ru" => "Russian",
        _ => "Korean", // 감지 실패/미지원 → 기본 한국어
    }
}

#[cfg(test)]
mod tests {
    use super::language_name;

    #[test]
    fn maps_common_locales() {
        assert_eq!(language_name("ko_KR.UTF-8"), "Korean");
        assert_eq!(language_name("en_US.UTF-8"), "English");
        assert_eq!(language_name("ja_JP"), "Japanese");
        assert_eq!(language_name("en-GB"), "English");
        assert_eq!(language_name("zh_CN.UTF-8"), "Chinese");
    }

    #[test]
    fn defaults_to_korean_on_unknown_or_empty() {
        assert_eq!(language_name(""), "Korean");
        assert_eq!(language_name("xx_YY"), "Korean");
        assert_eq!(language_name("C"), "Korean");
        assert_eq!(language_name("POSIX"), "Korean");
    }
}
