//! Script detection and token joining.

/// Returns true if a character falls within recognized CJK unicode ranges.
#[must_use]
pub fn is_cjk_char(c: char) -> bool {
    matches!(
        c,
        '\u{3040}'..='\u{309F}' | // Hiragana
        '\u{30A0}'..='\u{30FF}' | // Katakana
        '\u{4E00}'..='\u{9FFF}' | // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' | // CJK Extension A
        '\u{F900}'..='\u{FAFF}' | // CJK Compatibility Ideographs
        '\u{AC00}'..='\u{D7AF}' | // Hangul Syllables
        '\u{1100}'..='\u{11FF}' | // Hangul Jamo
        '\u{FF00}'..='\u{FFEF}' | // Halfwidth and Fullwidth Forms
        '\u{3000}'..='\u{303F}'   // CJK Symbols and Punctuation
    )
}

/// Returns true if any character in the text slice is CJK.
#[must_use]
pub fn is_cjk_text(s: &str) -> bool {
    s.chars().any(is_cjk_char)
}

/// Joins tokens using appropriate typography rules:
/// - Space-less `""` for CJK scripts (Japanese, Chinese, Korean), with spaces preserved between adjacent ASCII alphanumeric tokens.
/// - Space `" "` for Latin / alphabetic scripts.
#[must_use]
pub fn join_cjk_or_latin_tokens<S: AsRef<str>>(tokens: &[S], is_cjk: bool) -> String {
    if tokens.is_empty() {
        return String::new();
    }
    if !is_cjk {
        return tokens
            .iter()
            .map(|t| t.as_ref())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
    }

    let mut result = String::new();
    for (i, token) in tokens.iter().enumerate() {
        let t = token.as_ref().trim();
        if t.is_empty() {
            continue;
        }
        if i > 0 && !result.is_empty() {
            let prev_char = result.chars().last().unwrap_or(' ');
            let next_char = t.chars().next().unwrap_or(' ');
            // Insert space if transitioning between two ASCII alphanumeric tokens
            if prev_char.is_ascii_alphanumeric() && next_char.is_ascii_alphanumeric() {
                result.push(' ');
            }
        }
        result.push_str(t);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cjk_character_join_without_ascii_spaces() {
        // Japanese tokens: must be joined without spaces ("")
        let ja_tokens = vec!["私", "は", "学生", "です", "。"];
        let ja_joined = join_cjk_or_latin_tokens(&ja_tokens, true);
        assert_eq!(ja_joined, "私は学生です。");

        // Chinese tokens: must be joined without spaces ("")
        let zh_tokens = vec!["你好", "世界"];
        let zh_joined = join_cjk_or_latin_tokens(&zh_tokens, true);
        assert_eq!(zh_joined, "你好世界");

        // Latin / English tokens: must be joined with ASCII space (" ")
        let en_tokens = vec!["The", "hero", "entered", "the", "dungeon."];
        let en_joined = join_cjk_or_latin_tokens(&en_tokens, false);
        assert_eq!(en_joined, "The hero entered the dungeon.");

        // Mixed CJK + Western tokens: ASCII words retain word space, transition to CJK without space
        let mixed = vec!["Level", "10", "へ"];
        let mixed_joined = join_cjk_or_latin_tokens(&mixed, true);
        assert_eq!(mixed_joined, "Level 10へ");
    }

    #[test]
    fn test_cjk_char_detection() {
        assert!(is_cjk_char('あ')); // Hiragana
        assert!(is_cjk_char('ア')); // Katakana
        assert!(is_cjk_char('漢')); // Kanji
        assert!(is_cjk_char('한')); // Hangul
        assert!(is_cjk_char('。')); // CJK punctuation
        assert!(!is_cjk_char('A')); // Latin
        assert!(!is_cjk_char('1')); // Digit
        assert!(!is_cjk_char('?')); // ASCII punctuation

        assert!(is_cjk_text("レベルアップ!"));
        assert!(!is_cjk_text("Level Up!"));
    }
}
