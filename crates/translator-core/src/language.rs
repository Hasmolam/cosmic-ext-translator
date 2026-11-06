//! Source language selection and its mapping to OCR scripts.
//!
//! The applet stores the source language as a plain code string in `cosmic-config`.
//! This module normalises user-entered or legacy codes (`jp`, `JPN`, `japanese`, ...)
//! into a fixed set of languages the OCR pipeline knows how to handle.

/// Writing system that determines which OCR backend can read a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    Latin,
    Japanese,
    ChineseSimplified,
    Korean,
}

impl Script {
    #[must_use]
    pub fn is_cjk(self) -> bool {
        !matches!(self, Self::Latin)
    }

    /// Tesseract traineddata name for scripts that the bundled `ocrs` model cannot read.
    #[must_use]
    pub fn tesseract_pack(self) -> Option<&'static str> {
        match self {
            Self::Latin => None,
            Self::Japanese => Some("jpn"),
            Self::ChineseSimplified => Some("chi_sim"),
            Self::Korean => Some("kor"),
        }
    }

    /// English display name, used in logs and error messages.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Latin => "Latin",
            Self::Japanese => "Japanese",
            Self::ChineseSimplified => "Chinese (Simplified)",
            Self::Korean => "Korean",
        }
    }
}

/// Source languages offered in the settings UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceLanguage {
    Auto,
    English,
    Japanese,
    ChineseSimplified,
    Korean,
    German,
    French,
    Spanish,
    Italian,
    Portuguese,
    Turkish,
}

impl SourceLanguage {
    /// All selectable languages, in settings dropdown order.
    pub const ALL: [Self; 11] = [
        Self::Auto,
        Self::English,
        Self::Japanese,
        Self::ChineseSimplified,
        Self::Korean,
        Self::German,
        Self::French,
        Self::Spanish,
        Self::Italian,
        Self::Portuguese,
        Self::Turkish,
    ];

    /// Canonical code stored in config and passed to translation engines.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::English => "en",
            Self::Japanese => "ja",
            Self::ChineseSimplified => "zh",
            Self::Korean => "ko",
            Self::German => "de",
            Self::French => "fr",
            Self::Spanish => "es",
            Self::Italian => "it",
            Self::Portuguese => "pt",
            Self::Turkish => "tr",
        }
    }

    /// Parses a language code or English name, case-insensitively.
    ///
    /// An empty string maps to [`SourceLanguage::Auto`]. Unknown codes return `None`.
    #[must_use]
    pub fn from_code(raw: &str) -> Option<Self> {
        let code = raw.trim().to_ascii_lowercase().replace('_', "-");
        let lang = match code.as_str() {
            "" | "auto" => Self::Auto,
            "en" | "eng" | "english" => Self::English,
            "ja" | "jp" | "jpn" | "japanese" => Self::Japanese,
            "zh" | "zh-cn" | "zh-hans" | "chi" | "chi-sim" | "chinese" => Self::ChineseSimplified,
            "ko" | "kor" | "korean" => Self::Korean,
            "de" | "deu" | "ger" | "german" => Self::German,
            "fr" | "fra" | "fre" | "french" => Self::French,
            "es" | "spa" | "spanish" => Self::Spanish,
            "it" | "ita" | "italian" => Self::Italian,
            "pt" | "por" | "portuguese" => Self::Portuguese,
            "tr" | "tur" | "turkish" => Self::Turkish,
            _ => return None,
        };
        Some(lang)
    }

    /// Script the OCR stage has to read. `Auto` assumes Latin text.
    #[must_use]
    pub fn script(self) -> Script {
        match self {
            Self::Japanese => Script::Japanese,
            Self::ChineseSimplified => Script::ChineseSimplified,
            Self::Korean => Script::Korean,
            Self::Auto
            | Self::English
            | Self::German
            | Self::French
            | Self::Spanish
            | Self::Italian
            | Self::Portuguese
            | Self::Turkish => Script::Latin,
        }
    }

    /// Position in [`SourceLanguage::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }
}

/// Normalises a stored source-language string to the code sent to translation engines.
///
/// Known aliases collapse to their canonical code. Unknown codes are kept (trimmed and
/// lowercased) so a manually configured language is not discarded.
#[must_use]
pub fn normalize_source_code(raw: &str) -> String {
    match SourceLanguage::from_code(raw) {
        Some(lang) => lang.code().to_string(),
        None => raw.trim().to_ascii_lowercase(),
    }
}

/// Script for a stored source-language string. Unknown codes are treated as Latin.
#[must_use]
pub fn script_for_code(raw: &str) -> Script {
    SourceLanguage::from_code(raw).map_or(Script::Latin, SourceLanguage::script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_aliases_resolve() {
        for raw in ["ja", "JA", " ja ", "jp", "jpn", "Japanese"] {
            assert_eq!(
                SourceLanguage::from_code(raw),
                Some(SourceLanguage::Japanese),
                "{raw}"
            );
        }
    }

    #[test]
    fn empty_and_auto_are_auto() {
        assert_eq!(SourceLanguage::from_code(""), Some(SourceLanguage::Auto));
        assert_eq!(
            SourceLanguage::from_code("AUTO"),
            Some(SourceLanguage::Auto)
        );
    }

    #[test]
    fn chinese_variants_resolve() {
        for raw in ["zh", "zh-CN", "zh_CN", "chi_sim"] {
            assert_eq!(
                SourceLanguage::from_code(raw),
                Some(SourceLanguage::ChineseSimplified),
                "{raw}"
            );
        }
    }

    #[test]
    fn unknown_code_is_preserved_and_latin() {
        assert_eq!(SourceLanguage::from_code("nl"), None);
        assert_eq!(normalize_source_code(" NL "), "nl");
        assert_eq!(script_for_code("nl"), Script::Latin);
    }

    #[test]
    fn scripts_and_packs() {
        assert_eq!(script_for_code("jp"), Script::Japanese);
        assert!(Script::Japanese.is_cjk());
        assert!(!Script::Latin.is_cjk());
        assert_eq!(Script::Japanese.tesseract_pack(), Some("jpn"));
        assert_eq!(Script::Latin.tesseract_pack(), None);
        assert_eq!(script_for_code("auto"), Script::Latin);
    }

    #[test]
    fn codes_roundtrip_and_index() {
        for (i, lang) in SourceLanguage::ALL.iter().enumerate() {
            assert_eq!(SourceLanguage::from_code(lang.code()), Some(*lang));
            assert_eq!(lang.index(), i);
        }
    }
}
