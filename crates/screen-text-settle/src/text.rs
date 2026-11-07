//! Text-level deduplication after recognition.

use tracing::{debug, trace};

/// Decision of [`TextGate::evaluate`] for one recognised string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDiffAction {
    /// New text: a different line, or the first one. Process it now.
    EmitTranslation,
    /// The previous text with more characters appended: the line is still being typed.
    /// Wait, then take the full line from [`TextGate::flush_pending`].
    TypewriterWait,
    /// Empty, identical, or close enough to the last emitted text to be recognition
    /// noise. Ignore it.
    SuppressJitter,
}

/// Decides whether recognised text is new, still being typed, or noise.
///
/// Compares each string with the last one it emitted and the last one it saw:
///
/// 1. Empty or identical to the last candidate: [`TextDiffAction::SuppressJitter`].
/// 2. Starts with the last candidate or the last emitted text and is longer:
///    [`TextDiffAction::TypewriterWait`].
/// 3. Normalised Levenshtein similarity to the last emitted text at or above
///    `jitter_threshold`: [`TextDiffAction::SuppressJitter`].
/// 4. Otherwise: [`TextDiffAction::EmitTranslation`].
///
/// The gate keeps no clock. The caller decides how long to wait after a
/// `TypewriterWait` before calling [`TextGate::flush_pending`].
#[derive(Debug, Clone)]
pub struct TextGate {
    last_emitted_text: String,
    last_candidate_text: String,
    jitter_threshold: f64,
}

/// Jitter threshold 0.92.
impl Default for TextGate {
    fn default() -> Self {
        Self::new(0.92)
    }
}

impl TextGate {
    /// `jitter_threshold` is the similarity (0.0 to 1.0, clamped) at or above which a
    /// string counts as the last emitted text with recognition noise.
    #[must_use]
    pub fn new(jitter_threshold: f64) -> Self {
        Self {
            last_emitted_text: String::new(),
            last_candidate_text: String::new(),
            jitter_threshold: jitter_threshold.clamp(0.0, 1.0),
        }
    }

    /// Similarity at or above which a string counts as noise.
    #[must_use]
    pub fn jitter_threshold(&self) -> f64 {
        self.jitter_threshold
    }

    /// Classifies a recognised string. Leading and trailing whitespace is ignored.
    pub fn evaluate(&mut self, candidate: &str) -> TextDiffAction {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            return TextDiffAction::SuppressJitter;
        }

        // 1. Exact duplicate while waiting on typewriter: suppress without extending deadline
        if !self.last_candidate_text.is_empty() && trimmed == self.last_candidate_text {
            return TextDiffAction::SuppressJitter;
        }

        // 2. Monotonic prefix expansion check (Typewriter stream)
        // Must be evaluated BEFORE jitter threshold to avoid discarding 1-char expansions on long strings.
        let is_prefix_expansion = (!self.last_candidate_text.is_empty()
            && trimmed.starts_with(&self.last_candidate_text)
            && trimmed.len() > self.last_candidate_text.len())
            || (!self.last_emitted_text.is_empty()
                && trimmed.starts_with(&self.last_emitted_text)
                && trimmed.len() > self.last_emitted_text.len());

        if is_prefix_expansion {
            trace!(
                "TextGate: typewriter expansion detected ('{}' -> '{}')",
                if !self.last_candidate_text.is_empty() {
                    &self.last_candidate_text
                } else {
                    &self.last_emitted_text
                },
                trimmed
            );
            self.last_candidate_text = trimmed.to_string();
            return TextDiffAction::TypewriterWait;
        }

        // 3. Jitter suppression against last emitted text
        if !self.last_emitted_text.is_empty() {
            let sim = strsim::normalized_levenshtein(trimmed, &self.last_emitted_text);
            if sim >= self.jitter_threshold {
                trace!(
                    "TextGate: jitter suppressed (sim={:.3} >= {:.3}) for '{}'",
                    sim,
                    self.jitter_threshold,
                    trimmed
                );
                return TextDiffAction::SuppressJitter;
            }
        }

        // 4. New distinct line or dialogue turn (sim < jitter_threshold or initial text)
        debug!("TextGate: new text accepted for translation: '{}'", trimmed);
        self.last_emitted_text = trimmed.to_string();
        self.last_candidate_text = trimmed.to_string();
        TextDiffAction::EmitTranslation
    }

    /// Returns the longest line seen since the last emission, if it was not emitted yet,
    /// and records it as emitted.
    pub fn flush_pending(&mut self) -> Option<String> {
        if self.last_candidate_text.is_empty() || self.last_candidate_text == self.last_emitted_text
        {
            None
        } else {
            self.last_emitted_text = self.last_candidate_text.clone();
            Some(self.last_emitted_text.clone())
        }
    }

    /// Whether a typed line is waiting for [`Self::flush_pending`].
    #[must_use]
    pub fn has_pending_expansion(&self) -> bool {
        !self.last_candidate_text.is_empty() && self.last_candidate_text != self.last_emitted_text
    }

    /// Forgets the emitted and pending text.
    pub fn reset(&mut self) {
        self.last_emitted_text.clear();
        self.last_candidate_text.clear();
    }

    /// Last text returned as new or flushed.
    #[must_use]
    pub fn last_emitted_text(&self) -> &str {
        &self.last_emitted_text
    }

    /// Last text that was new or extended the previous one.
    #[must_use]
    pub fn last_candidate_text(&self) -> &str {
        &self.last_candidate_text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_gate_jitter_suppression() {
        let mut gate = TextGate::new(0.92);
        let base_text = "Press the button to initiate warp jump sequence.";

        // 1. Initial emission
        assert_eq!(gate.evaluate(base_text), TextDiffAction::EmitTranslation);
        assert_eq!(gate.last_emitted_text(), base_text);

        // 2. Exact duplicate
        assert_eq!(gate.evaluate(base_text), TextDiffAction::SuppressJitter);

        // 3. Leading/trailing whitespace
        assert_eq!(
            gate.evaluate("   Press the button to initiate warp jump sequence.   "),
            TextDiffAction::SuppressJitter
        );

        // 4. Minor OCR punctuation jitter (missing period: sim ~0.979 >= 0.92)
        assert_eq!(
            gate.evaluate("Press the button to initiate warp jump sequence"),
            TextDiffAction::SuppressJitter
        );

        // 5. Minor typo substitution (comma instead of period: sim ~0.979 >= 0.92)
        assert_eq!(
            gate.evaluate("Press the button to initiate warp jump sequence,"),
            TextDiffAction::SuppressJitter
        );

        // 6. Empty string
        assert_eq!(gate.evaluate("   "), TextDiffAction::SuppressJitter);

        // Assert state is untouched
        assert_eq!(gate.last_emitted_text(), base_text);
        assert_eq!(gate.flush_pending(), None);
    }

    #[test]
    fn test_text_gate_typewriter_progressive_expansion() {
        let mut gate = TextGate::new(0.92);

        // 1. Initial word appears
        assert_eq!(gate.evaluate("Wait"), TextDiffAction::EmitTranslation);
        assert_eq!(gate.last_emitted_text(), "Wait");
        assert_eq!(gate.last_candidate_text(), "Wait");

        // 2. Progressive typewriter expansion 1
        assert_eq!(gate.evaluate("Wait, don't"), TextDiffAction::TypewriterWait);
        assert_eq!(gate.last_emitted_text(), "Wait");
        assert_eq!(gate.last_candidate_text(), "Wait, don't");
        assert!(gate.has_pending_expansion());

        // 3. Progressive typewriter expansion 2
        assert_eq!(
            gate.evaluate("Wait, don't open"),
            TextDiffAction::TypewriterWait
        );
        assert_eq!(gate.last_candidate_text(), "Wait, don't open");

        // 4. Progressive typewriter expansion 3 (final sentence text)
        assert_eq!(
            gate.evaluate("Wait, don't open that door!"),
            TextDiffAction::TypewriterWait
        );
        assert_eq!(gate.last_candidate_text(), "Wait, don't open that door!");

        // 5. Duplicate candidate while waiting (game finished typing, screen idle)
        assert_eq!(
            gate.evaluate("Wait, don't open that door!"),
            TextDiffAction::SuppressJitter
        );

        // 6. Timeout expires: flush settled text
        let flushed = gate.flush_pending();
        assert_eq!(flushed, Some("Wait, don't open that door!".to_string()));
        assert_eq!(gate.last_emitted_text(), "Wait, don't open that door!");
        assert_eq!(gate.flush_pending(), None);
    }

    #[test]
    fn test_text_gate_dialogue_turn_transition() {
        let mut gate = TextGate::new(0.92);

        // Turn 1
        let turn1 = "What a wonderful evening for a stroll through the forest.";
        assert_eq!(gate.evaluate(turn1), TextDiffAction::EmitTranslation);
        assert_eq!(gate.last_emitted_text(), turn1);

        // Jitter on Turn 1
        assert_eq!(
            gate.evaluate("What a wonderful evening for a stroll through the forest"),
            TextDiffAction::SuppressJitter
        );

        // Turn 2: completely distinct sentence from Character B (sim < 0.60)
        let turn2 = "Look out! Goblins are attacking from the shadows!";
        assert_eq!(gate.evaluate(turn2), TextDiffAction::EmitTranslation);
        assert_eq!(gate.last_emitted_text(), turn2);

        // Turn 3: short exclamation
        let turn3 = "Draw your sword";
        assert_eq!(gate.evaluate(turn3), TextDiffAction::EmitTranslation);
        assert_eq!(gate.last_emitted_text(), turn3);

        // Expansion on Turn 3
        assert_eq!(
            gate.evaluate("Draw your sword and defend yourself!"),
            TextDiffAction::TypewriterWait
        );
        assert_eq!(
            gate.flush_pending(),
            Some("Draw your sword and defend yourself!".to_string())
        );
    }

    #[test]
    fn test_text_gate_cjk_support() {
        let mut gate = TextGate::default();

        // Japanese dialogue prefix expansion
        assert_eq!(
            gate.evaluate("「こんにちは"),
            TextDiffAction::EmitTranslation
        );
        assert_eq!(
            gate.evaluate("「こんにちは、世界」"),
            TextDiffAction::TypewriterWait
        );
        assert_eq!(
            gate.flush_pending(),
            Some("「こんにちは、世界」".to_string())
        );

        // Jitter on settled Japanese string
        assert_eq!(
            gate.evaluate("「こんにちは、世界」"),
            TextDiffAction::SuppressJitter
        );

        // Dialogue turn change
        assert_eq!(
            gate.evaluate("【勇者】魔王を倒す時が来た！"),
            TextDiffAction::EmitTranslation
        );
    }
}
