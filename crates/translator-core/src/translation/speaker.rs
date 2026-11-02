use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Supported bracket styles for visual novel and RPG speaker nameplates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BracketStyle {
    /// Japanese lenticular brackets: 【 ... 】
    Lenticular,
    /// Tortoise shell brackets: 〔 ... 〕
    TortoiseShell,
    /// Japanese corner quotes: 「 ... 」
    CornerBracket,
    /// Standard ASCII square brackets: [ ... ]
    SquareBracket,
    /// Fullwidth square brackets: ［ ... ］
    FullwidthSquare,
    /// White lenticular brackets: 〖 ... 〗
    WhiteLenticular,
    /// Colon nameplate delimiter: Name: or Name：
    ColonNameplate,
}

impl BracketStyle {
    /// Formats a speaker name using this bracket style.
    pub fn wrap(&self, speaker: &str) -> String {
        match self {
            Self::Lenticular => format!("【{speaker}】"),
            Self::TortoiseShell => format!("〔{speaker}〕"),
            Self::CornerBracket => format!("「{speaker}」"),
            Self::SquareBracket => format!("[{speaker}]"),
            Self::FullwidthSquare => format!("［{speaker}］"),
            Self::WhiteLenticular => format!("〖{speaker}〗"),
            Self::ColonNameplate => format!("{speaker}: "),
        }
    }
}

/// Represents a parsed dialogue line with extracted speaker tag metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedDialogueLine {
    pub raw_speaker: Option<String>,
    pub resolved_speaker: Option<String>,
    pub dialogue: String,
    pub bracket_style: Option<BracketStyle>,
}

/// Parses speaker tags from a raw OCR string and resolves them against a glossary.
///
/// Strips the speaker nameplate from the returned dialogue so only the dialogue
/// text is forwarded to the translation backend.
pub fn parse_speaker_tags(raw: &str, glossary: &HashMap<String, String>) -> ParsedDialogueLine {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ParsedDialogueLine {
            raw_speaker: None,
            resolved_speaker: None,
            dialogue: String::new(),
            bracket_style: None,
        };
    }

    // Bracket delimiters: (open, close, style)
    let bracket_pairs = [
        ('【', '】', BracketStyle::Lenticular),
        ('〔', '〕', BracketStyle::TortoiseShell),
        ('「', '」', BracketStyle::CornerBracket),
        ('[', ']', BracketStyle::SquareBracket),
        ('［', '］', BracketStyle::FullwidthSquare),
        ('〖', '〗', BracketStyle::WhiteLenticular),
    ];

    for (open, close, style) in bracket_pairs {
        if trimmed.starts_with(open) {
            let after_open = &trimmed[open.len_utf8()..];
            if let Some(close_idx) = after_open.find(close) {
                let speaker = after_open[..close_idx].trim();
                let mut dialogue = after_open[close_idx + close.len_utf8()..].trim();

                // Strip optional colon following bracket nameplate, e.g. "[Speaker]: dialogue"
                if let Some(stripped) = dialogue.strip_prefix(':') {
                    dialogue = stripped.trim();
                } else if let Some(stripped) = dialogue.strip_prefix('：') {
                    dialogue = stripped.trim();
                }

                if !speaker.is_empty() && (!dialogue.is_empty() || glossary.contains_key(speaker)) {
                    let resolved = glossary
                        .get(speaker)
                        .cloned()
                        .unwrap_or_else(|| speaker.to_string());
                    return ParsedDialogueLine {
                        raw_speaker: Some(speaker.to_string()),
                        resolved_speaker: Some(resolved),
                        dialogue: dialogue.to_string(),
                        bracket_style: Some(style),
                    };
                }
            }
        }
    }

    // Colon delimiters (ASCII ':' and Japanese fullwidth '：')
    for colon in [':', '：'] {
        if let Some(colon_pos) = trimmed.find(colon) {
            // Speaker nameplates are typically concise (< 25 chars) and single-line
            if colon_pos > 0 && colon_pos <= 25 {
                let potential_speaker = trimmed[..colon_pos].trim();
                let potential_dialogue = trimmed[colon_pos + colon.len_utf8()..].trim();

                let is_url = potential_dialogue.starts_with("//");
                let is_timestamp = potential_speaker.chars().all(|c| c.is_ascii_digit());

                if !potential_speaker.is_empty()
                    && !potential_dialogue.is_empty()
                    && !potential_speaker.contains('\n')
                    && !is_url
                    && !is_timestamp
                {
                    let resolved = glossary
                        .get(potential_speaker)
                        .cloned()
                        .unwrap_or_else(|| potential_speaker.to_string());
                    return ParsedDialogueLine {
                        raw_speaker: Some(potential_speaker.to_string()),
                        resolved_speaker: Some(resolved),
                        dialogue: potential_dialogue.to_string(),
                        bracket_style: Some(BracketStyle::ColonNameplate),
                    };
                }
            }
        }
    }

    ParsedDialogueLine {
        raw_speaker: None,
        resolved_speaker: None,
        dialogue: trimmed.to_string(),
        bracket_style: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bracket_style_wrap() {
        assert_eq!(BracketStyle::Lenticular.wrap("Alice"), "【Alice】");
        assert_eq!(BracketStyle::TortoiseShell.wrap("Bob"), "〔Bob〕");
        assert_eq!(BracketStyle::CornerBracket.wrap("Carol"), "「Carol」");
        assert_eq!(BracketStyle::SquareBracket.wrap("Dave"), "[Dave]");
        assert_eq!(BracketStyle::FullwidthSquare.wrap("Eve"), "［Eve］");
        assert_eq!(BracketStyle::WhiteLenticular.wrap("Frank"), "〖Frank〗");
        assert_eq!(BracketStyle::ColonNameplate.wrap("Grace"), "Grace: ");
    }

    #[test]
    fn test_parse_speaker_tags_lenticular() {
        let mut glossary = HashMap::new();
        glossary.insert("アリス".to_string(), "Alice".to_string());

        let parsed = parse_speaker_tags("【アリス】 気をつけてね。", &glossary);
        assert_eq!(parsed.raw_speaker.as_deref(), Some("アリス"));
        assert_eq!(parsed.resolved_speaker.as_deref(), Some("Alice"));
        assert_eq!(parsed.dialogue, "気をつけてね。");
        assert_eq!(parsed.bracket_style, Some(BracketStyle::Lenticular));
    }

    #[test]
    fn test_parse_speaker_tags_colon() {
        let mut glossary = HashMap::new();
        glossary.insert("Hero".to_string(), "Kahraman".to_string());

        let parsed = parse_speaker_tags("Hero: Let's move out!", &glossary);
        assert_eq!(parsed.raw_speaker.as_deref(), Some("Hero"));
        assert_eq!(parsed.resolved_speaker.as_deref(), Some("Kahraman"));
        assert_eq!(parsed.dialogue, "Let's move out!");
        assert_eq!(parsed.bracket_style, Some(BracketStyle::ColonNameplate));
    }

    #[test]
    fn test_parse_speaker_tags_unbracketed() {
        let glossary = HashMap::new();
        let parsed = parse_speaker_tags("Just normal narration here.", &glossary);
        assert!(parsed.raw_speaker.is_none());
        assert!(parsed.resolved_speaker.is_none());
        assert_eq!(parsed.dialogue, "Just normal narration here.");
        assert!(parsed.bracket_style.is_none());
    }
}
