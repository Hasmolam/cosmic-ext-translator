use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a morphological token card with lemma, reading, POS, and definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MorphemeCard {
    pub surface: String,
    pub base_form: String,
    pub reading: String,
    pub part_of_speech: String,
    pub definitions: Vec<String>,
}

/// Japanese morphological dictionary providing greedy longest-match tokenization
/// and lexical definition lookups.
#[derive(Debug, Clone, Default)]
pub struct JapaneseDictionary {
    lexicon: HashMap<String, MorphemeCard>,
}

impl JapaneseDictionary {
    /// Creates a new `JapaneseDictionary` populated with common Japanese visual novel / RPG vocabulary.
    pub fn new() -> Self {
        let mut dict = Self {
            lexicon: HashMap::new(),
        };
        dict.load_core_vocabulary();
        dict
    }

    /// Creates an empty `JapaneseDictionary` without preloaded vocabulary.
    pub fn empty() -> Self {
        Self {
            lexicon: HashMap::new(),
        }
    }

    /// Inserts a morpheme entry into the dictionary.
    pub fn insert(
        &mut self,
        surface: impl Into<String>,
        reading: impl Into<String>,
        pos: impl Into<String>,
        defs: Vec<String>,
    ) {
        let s = surface.into();
        self.lexicon.insert(
            s.clone(),
            MorphemeCard {
                surface: s.clone(),
                base_form: s,
                reading: reading.into(),
                part_of_speech: pos.into(),
                definitions: defs,
            },
        );
    }

    /// Inserts a morpheme entry specifying an explicit base form.
    pub fn insert_with_base_form(
        &mut self,
        surface: impl Into<String>,
        base_form: impl Into<String>,
        reading: impl Into<String>,
        pos: impl Into<String>,
        defs: Vec<String>,
    ) {
        let s = surface.into();
        self.lexicon.insert(
            s.clone(),
            MorphemeCard {
                surface: s,
                base_form: base_form.into(),
                reading: reading.into(),
                part_of_speech: pos.into(),
                definitions: defs,
            },
        );
    }

    /// Looks up a specific surface form directly in the lexicon.
    pub fn lookup(&self, surface: &str) -> Option<&MorphemeCard> {
        self.lexicon.get(surface)
    }

    /// Analyzes Japanese text using greedy longest-match tokenization.
    ///
    /// Matches the longest known dictionary prefix at each character boundary
    /// and returns the corresponding morpheme cards in reading order.
    pub fn analyze(&self, text: &str) -> Vec<MorphemeCard> {
        let mut results = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let mut matched = false;
            let max_len = (chars.len() - i).min(16);

            for len in (1..=max_len).rev() {
                let sub: String = chars[i..i + len].iter().collect();
                if let Some(card) = self.lexicon.get(&sub) {
                    results.push(card.clone());
                    i += len;
                    matched = true;
                    break;
                }
            }

            if !matched {
                i += 1;
            }
        }

        results
    }

    /// Returns the number of lexical entries currently registered.
    pub fn len(&self) -> usize {
        self.lexicon.len()
    }

    /// Returns true if the dictionary has no entries.
    pub fn is_empty(&self) -> bool {
        self.lexicon.is_empty()
    }

    /// Clears all entries from the dictionary.
    pub fn clear(&mut self) {
        self.lexicon.clear();
    }

    /// Preloads common Japanese gaming and dialogue vocabulary for offline zero-network operation.
    fn load_core_vocabulary(&mut self) {
        let core_entries: [(&str, &str, &str, &[&str]); 15] = [
            ("私", "わたし", "Pronoun", &["I", "me"]),
            ("僕", "ぼく", "Pronoun", &["I (male)", "me"]),
            ("俺", "おれ", "Pronoun", &["I (informal male)", "me"]),
            ("勇者", "ゆうしゃ", "Noun", &["hero", "brave person"]),
            ("魔法", "まほう", "Noun", &["magic", "sorcery"]),
            ("剣", "つるぎ", "Noun", &["sword", "blade"]),
            ("盾", "たて", "Noun", &["shield"]),
            ("敵", "てき", "Noun", &["enemy", "opponent"]),
            (
                "回復",
                "かいふく",
                "Noun, Suru Verb",
                &["recovery", "healing", "restoration"],
            ),
            (
                "気をつけて",
                "きをつけて",
                "Expression",
                &["take care", "be careful", "watch out"],
            ),
            ("ありがとう", "ありがとう", "Expression", &["thank you"]),
            (
                "さようなら",
                "さようなら",
                "Expression",
                &["goodbye", "farewell"],
            ),
            (
                "大丈夫",
                "だいじょうぶ",
                "Adjective",
                &["all right", "okay", "fine"],
            ),
            ("攻撃", "こうげき", "Noun, Suru Verb", &["attack", "strike"]),
            (
                "防御",
                "ぼうぎょ",
                "Noun, Suru Verb",
                &["defense", "protection"],
            ),
        ];

        for (surf, reading, pos, defs) in core_entries {
            self.insert(
                surf,
                reading,
                pos,
                defs.iter().map(|s| s.to_string()).collect(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dictionary_longest_match() {
        let mut dict = JapaneseDictionary::empty();
        dict.insert("勇者", "ゆうしゃ", "Noun", vec!["hero".to_string()]);
        dict.insert(
            "勇者様",
            "ゆうしゃさま",
            "Noun",
            vec!["Lord Hero".to_string()],
        );

        let cards = dict.analyze("勇者様が来た");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].surface, "勇者様");
        assert_eq!(cards[0].reading, "ゆうしゃさま");
        assert_eq!(cards[0].definitions, vec!["Lord Hero"]);
    }

    #[test]
    fn test_core_vocabulary_analysis() {
        let dict = JapaneseDictionary::new();
        let cards = dict.analyze("勇者が魔法で攻撃する");
        let surfaces: Vec<&str> = cards.iter().map(|c| c.surface.as_str()).collect();
        assert!(surfaces.contains(&"勇者"));
        assert!(surfaces.contains(&"魔法"));
        assert!(surfaces.contains(&"攻撃"));
    }
}
