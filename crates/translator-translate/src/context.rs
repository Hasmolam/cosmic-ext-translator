use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

/// A single dialogue turn recorded in the sliding context history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DialogueTurn {
    pub speaker: Option<String>,
    pub source: String,
    pub target: String,
    pub timestamp_millis: u64,
}

/// Context for translation including recent dialogue history and game glossary.
#[derive(Debug, Clone)]
pub struct TranslationContext {
    history: VecDeque<DialogueTurn>,
    max_history: usize,
    glossary: HashMap<String, String>,
}

impl Default for TranslationContext {
    fn default() -> Self {
        Self::new(4)
    }
}

impl TranslationContext {
    /// Creates a new `TranslationContext` retaining up to `max_history` dialogue turns.
    pub fn new(max_history: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
            glossary: HashMap::new(),
        }
    }

    /// Records a completed dialogue turn into history without a speaker tag.
    pub fn add_turn(&mut self, original: impl Into<String>, translated: impl Into<String>) {
        self.add_turn_with_speaker(None::<String>, original, translated);
    }

    /// Records a completed dialogue turn into history with an optional speaker tag.
    pub fn add_turn_with_speaker(
        &mut self,
        speaker: Option<impl Into<String>>,
        original: impl Into<String>,
        translated: impl Into<String>,
    ) {
        if self.max_history == 0 {
            return;
        }
        if self.history.len() >= self.max_history {
            self.history.pop_back();
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        self.history.push_front(DialogueTurn {
            speaker: speaker.map(Into::into),
            source: original.into(),
            target: translated.into(),
            timestamp_millis: now,
        });
    }

    /// Adds or updates a term in the domain-specific glossary.
    pub fn set_glossary_term(&mut self, term: impl Into<String>, translation: impl Into<String>) {
        self.glossary.insert(term.into(), translation.into());
    }

    /// Returns the recent dialogue history (most recent first).
    pub fn history(&self) -> &VecDeque<DialogueTurn> {
        &self.history
    }

    /// Returns the glossary terms.
    pub fn glossary(&self) -> &HashMap<String, String> {
        &self.glossary
    }

    /// Clears dialogue history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Builds an LLM system prompt containing game localization instructions,
    /// glossary rules, and recent dialogue history for grammatical/gender continuity.
    pub fn build_system_prompt(&self, src_lang: &str, target_lang: &str) -> String {
        self.build_system_prompt_with_speaker(src_lang, target_lang, None)
    }

    /// Builds an LLM system prompt including character glossary entries, previous
    /// turns from oldest to newest with speaker tags, and current line speaker attribution.
    pub fn build_system_prompt_with_speaker(
        &self,
        src_lang: &str,
        target_lang: &str,
        current_speaker: Option<&str>,
    ) -> String {
        let mut prompt = format!(
            "You are a professional video game localization engine.\n\
             Translate the provided dialogue text from {src_lang} to {target_lang}.\n\
             Rules:\n\
             1. Output ONLY the translated dialogue text without quotes or explanations.\n\
             2. Maintain pronoun consistency, grammatical gender, and conversational flow based on context.\n"
        );

        if let Some(spk) = current_speaker {
            prompt.push_str(&format!("\nCurrent Line Speaker: \"{spk}\"\n"));
        }

        if !self.glossary.is_empty() {
            prompt.push_str("\n[Glossary / Preserved Proper Nouns]:\n");
            let mut entries: Vec<(&String, &String)> = self.glossary.iter().collect();
            entries.sort_by_key(|(k, _)| *k);
            for (term, repl) in entries {
                prompt.push_str(&format!("- \"{term}\" -> \"{repl}\"\n"));
            }
        }

        if !self.history.is_empty() {
            prompt.push_str("\n[Previous Dialogue Turns (Oldest to Newest)]:\n");
            for turn in self.history.iter().rev() {
                let spk = turn.speaker.as_deref().unwrap_or("Dialogue");
                prompt.push_str(&format!(
                    "{spk}: \"{}\" => \"{}\"\n",
                    turn.source, turn.target
                ));
            }
        }

        prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_history_rotation() {
        let mut ctx = TranslationContext::new(2);
        ctx.add_turn("Turn 1", "Tur 1");
        ctx.add_turn("Turn 2", "Tur 2");
        assert_eq!(ctx.history().len(), 2);

        ctx.add_turn("Turn 3", "Tur 3");
        assert_eq!(ctx.history().len(), 2);
        assert_eq!(ctx.history()[0].source, "Turn 3");
        assert_eq!(ctx.history()[1].source, "Turn 2");
    }

    #[test]
    fn test_prompt_builder_with_speaker() {
        let mut ctx = TranslationContext::new(3);
        ctx.set_glossary_term("Chocobo", "Kocakuş");
        ctx.add_turn_with_speaker(Some("Cloud"), "Look over there!", "Şuraya bak!");

        let prompt = ctx.build_system_prompt_with_speaker("en", "tr", Some("Tifa"));
        assert!(prompt.contains("Chocobo"));
        assert!(prompt.contains("Kocakuş"));
        assert!(prompt.contains("Current Line Speaker: \"Tifa\""));
        assert!(prompt.contains("Cloud: \"Look over there!\" => \"Şuraya bak!\""));
    }
}
