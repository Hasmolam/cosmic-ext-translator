use image::RgbaImage;
use std::time::{Duration, Instant};
use tracing::{debug, trace};

use crate::pipeline::phash::{dhash_256, hamming_distance_256};

/// A frame the debouncer can hash. Implemented for plain images and for capture
/// types that carry extra metadata alongside the pixels.
pub trait FrameImage {
    fn image(&self) -> &RgbaImage;
}

impl FrameImage for RgbaImage {
    fn image(&self) -> &RgbaImage {
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebounceState {
    /// Screen is stable, no active text writing detected.
    Idle,
    /// pHash differences detected; dialogue is streaming/typing.
    Animating,
    /// Dialogue text has finished typing and stabilized.
    Settled,
}

#[derive(Debug, Clone)]
pub struct SettledFrame<F> {
    pub frame: F,
    pub hash: u64,
    pub settled_at: Instant,
}

pub struct FrameDebouncer<F> {
    /// Hamming distance threshold to treat a frame as changed.
    pub hash_threshold: u32,
    /// Time without hash changes required to declare text settled.
    pub settle_duration: Duration,
    /// Maximum time in animating state before forcing settled state.
    pub max_animation_duration: Duration,

    state: DebounceState,
    last_hash: Option<[u64; 4]>,
    last_change_time: Instant,
    animation_start_time: Option<Instant>,
    latest_frame: Option<F>,
}

impl<F: FrameImage> Default for FrameDebouncer<F> {
    fn default() -> Self {
        Self::new(3, Duration::from_millis(180), Duration::from_millis(1500))
    }
}

impl<F: FrameImage> FrameDebouncer<F> {
    #[must_use]
    pub fn new(
        hash_threshold: u32,
        settle_duration: Duration,
        max_animation_duration: Duration,
    ) -> Self {
        Self {
            hash_threshold,
            settle_duration,
            max_animation_duration,
            state: DebounceState::Idle,
            last_hash: None,
            last_change_time: Instant::now(),
            animation_start_time: None,
            latest_frame: None,
        }
    }

    #[must_use]
    pub fn state(&self) -> DebounceState {
        self.state
    }

    /// Ingests a new captured frame, timestamped with the current time.
    ///
    /// Returns `Some(SettledFrame)` only when the text stream has completed typing
    /// and remained visually stable for `settle_duration`.
    pub fn process_frame(&mut self, frame: F) -> Option<SettledFrame<F>> {
        self.process_frame_at(frame, Instant::now())
    }

    /// Same as [`Self::process_frame`] with an explicit timestamp, for callers that
    /// replay recorded frames or drive the debouncer from a simulated clock.
    pub fn process_frame_at(&mut self, frame: F, now: Instant) -> Option<SettledFrame<F>> {
        let current_hash = dhash_256(frame.image());

        match self.state {
            DebounceState::Idle => {
                let Some(prev_hash) = self.last_hash else {
                    // Very first frame: establish baseline and stay Idle
                    self.last_hash = Some(current_hash);
                    return None;
                };

                let distance = hamming_distance_256(&prev_hash, &current_hash);
                trace!(
                    "Debouncer: Idle, dist={}, threshold={}",
                    distance,
                    self.hash_threshold
                );

                if distance >= self.hash_threshold {
                    debug!("Debouncer: change detected (dist={distance}), entering Animating");
                    self.state = DebounceState::Animating;
                    self.animation_start_time = Some(now);
                    self.last_change_time = now;
                    self.last_hash = Some(current_hash);
                    self.latest_frame = Some(frame);
                }
                None
            }

            DebounceState::Animating => {
                let prev_hash = self.last_hash.unwrap_or(current_hash);
                let distance = hamming_distance_256(&prev_hash, &current_hash);
                self.latest_frame = Some(frame);

                // Only treat changes with distance >= hash_threshold as active text writing.
                // Sub-threshold fluctuations are subpixel rasterization jitter or idle background sprite breathing.
                if distance >= self.hash_threshold {
                    trace!("Debouncer: still actively typing (dist={distance})");
                    self.last_hash = Some(current_hash);
                    self.last_change_time = now;
                }

                let elapsed_since_change = now.duration_since(self.last_change_time);
                let elapsed_since_anim_start = self
                    .animation_start_time
                    .map(|t| now.duration_since(t))
                    .unwrap_or_default();

                let has_settled = elapsed_since_change >= self.settle_duration;
                let reached_max_time = elapsed_since_anim_start >= self.max_animation_duration;

                if has_settled || reached_max_time {
                    debug!(
                        "Debouncer: text settled! (waited={:?}, max_reached={})",
                        elapsed_since_change, reached_max_time
                    );

                    self.state = DebounceState::Settled;
                    self.animation_start_time = None;

                    let final_frame = self.latest_frame.take()?;
                    let compact_hash =
                        current_hash[0] ^ current_hash[1] ^ current_hash[2] ^ current_hash[3];
                    let settled = SettledFrame {
                        frame: final_frame,
                        hash: compact_hash,
                        settled_at: now,
                    };

                    // Transition back to Idle for the next dialogue line
                    self.state = DebounceState::Idle;
                    self.last_hash = Some(current_hash);

                    Some(settled)
                } else {
                    None
                }
            }

            DebounceState::Settled => {
                // Resets to Idle
                self.state = DebounceState::Idle;
                None
            }
        }
    }

    /// Forces a settled state if the animation is pending.
    /// This resolves zero-damage / 0-FPS game engine freezes where the text has stopped
    /// animating but the game engine produces no further damaged frames to trigger the watch channel.
    pub fn force_settle(&mut self) -> Option<SettledFrame<F>> {
        if self.state == DebounceState::Animating {
            let final_frame = self.latest_frame.take()?;
            let hash = self.last_hash.unwrap_or_default();
            let compact_hash = hash[0] ^ hash[1] ^ hash[2] ^ hash[3];
            self.state = DebounceState::Idle;
            self.animation_start_time = None;
            Some(SettledFrame {
                frame: final_frame,
                hash: compact_hash,
                settled_at: Instant::now(),
            })
        } else {
            None
        }
    }

    /// Resets internal state (useful when changing ROI or switching game windows).
    pub fn reset(&mut self) {
        self.state = DebounceState::Idle;
        self.last_hash = None;
        self.latest_frame = None;
        self.animation_start_time = None;
    }
}

/// Action decided by the Stage-2 post-OCR text debouncer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDiffAction {
    /// Distinct text change or new dialogue turn; emit to translation and HUD immediately.
    EmitTranslation,
    /// Progressive typewriter expansion; hold emission and wait for text to finish typing.
    TypewriterWait,
    /// Micro-noise, identical string, or animation particle flutter; suppress emission.
    SuppressJitter,
}

/// Stage-2 post-OCR text-level gating component.
///
/// Evaluates candidate strings recognized by OCR against previously emitted and candidate
/// text to suppress rasterization jitter, avoid duplicate translation requests, and prevent
/// subtitle stutter during progressive typewriter text streaming.
#[derive(Debug, Clone)]
pub struct TextGate {
    pub last_emitted_text: String,
    pub last_candidate_text: String,
    pub last_candidate_time: Instant,
    pub typewriter_settle_duration: Duration,
    pub jitter_threshold: f64,
    pub turn_threshold: f64,
}

impl Default for TextGate {
    fn default() -> Self {
        Self::new(Duration::from_millis(250), 0.92, 0.60)
    }
}

impl TextGate {
    #[must_use]
    pub fn new(
        typewriter_settle_duration: Duration,
        jitter_threshold: f64,
        turn_threshold: f64,
    ) -> Self {
        Self {
            last_emitted_text: String::new(),
            last_candidate_text: String::new(),
            last_candidate_time: Instant::now(),
            typewriter_settle_duration,
            jitter_threshold: jitter_threshold.clamp(0.0, 1.0),
            turn_threshold: turn_threshold.clamp(0.0, 1.0),
        }
    }

    /// Evaluates a candidate string from OCR and determines whether to emit, hold, or suppress.
    pub fn evaluate(&mut self, candidate: &str) -> TextDiffAction {
        self.evaluate_at(candidate, Instant::now())
    }

    /// Evaluates a candidate string with an explicit timestamp (for deterministic headless unit testing).
    pub fn evaluate_at(&mut self, candidate: &str, now: Instant) -> TextDiffAction {
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
            self.last_candidate_time = now;
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
        self.last_candidate_time = now;
        TextDiffAction::EmitTranslation
    }

    /// Flushes any pending typewriter candidate that has finished expanding.
    ///
    /// If `last_candidate_text` has expanded beyond `last_emitted_text`, commits
    /// it to `last_emitted_text` and returns `Some(text)`.
    pub fn flush_pending(&mut self) -> Option<String> {
        if self.last_candidate_text.is_empty() || self.last_candidate_text == self.last_emitted_text
        {
            None
        } else {
            self.last_emitted_text = self.last_candidate_text.clone();
            Some(self.last_emitted_text.clone())
        }
    }

    /// Checks if there is a pending typewriter expansion awaiting settle timeout.
    #[must_use]
    pub fn has_pending_expansion(&self) -> bool {
        !self.last_candidate_text.is_empty() && self.last_candidate_text != self.last_emitted_text
    }

    /// Resets the internal state (useful when switching game windows or changing ROI).
    pub fn reset(&mut self) {
        self.last_emitted_text.clear();
        self.last_candidate_text.clear();
        self.last_candidate_time = Instant::now();
    }

    #[must_use]
    pub fn last_emitted_text(&self) -> &str {
        &self.last_emitted_text
    }

    #[must_use]
    pub fn last_candidate_text(&self) -> &str {
        &self.last_candidate_text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn make_frame_with_blocks(num_blocks: u32) -> RgbaImage {
        let mut img = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
        for i in 0..num_blocks {
            let start_x = (i * 12).min(90);
            for y in 10..40 {
                for x in start_x..(start_x + 8).min(100) {
                    img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
                }
            }
        }
        img
    }

    #[test]
    fn test_debouncer_typewriter_simulation() {
        let mut debouncer =
            FrameDebouncer::new(2, Duration::from_millis(50), Duration::from_millis(500));

        // Frame 0: Baseline empty box
        assert!(debouncer.process_frame(make_frame_with_blocks(0)).is_none());
        assert_eq!(debouncer.state(), DebounceState::Idle);

        // Frame 1: Text typing starts -> Animating
        assert!(debouncer.process_frame(make_frame_with_blocks(1)).is_none());
        assert_eq!(debouncer.state(), DebounceState::Animating);

        // Frames 2..3: Rapid typing changes (still animating, no settle)
        assert!(debouncer.process_frame(make_frame_with_blocks(2)).is_none());
        assert_eq!(debouncer.state(), DebounceState::Animating);

        // Wait past the 50ms settle duration
        std::thread::sleep(Duration::from_millis(65));

        // Next frame with same image: should trigger Settled and return Some!
        let settled = debouncer.process_frame(make_frame_with_blocks(2));
        assert!(
            settled.is_some(),
            "Debouncer should emit settled frame after stability window"
        );
        assert_eq!(debouncer.state(), DebounceState::Idle);
    }

    #[test]
    fn test_text_gate_jitter_suppression() {
        let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);
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
        let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);
        let t0 = Instant::now();

        // 1. Initial word appears
        assert_eq!(
            gate.evaluate_at("Wait", t0),
            TextDiffAction::EmitTranslation
        );
        assert_eq!(gate.last_emitted_text(), "Wait");
        assert_eq!(gate.last_candidate_text(), "Wait");

        // 2. Progressive typewriter expansion 1
        assert_eq!(
            gate.evaluate_at("Wait, don't", t0 + Duration::from_millis(50)),
            TextDiffAction::TypewriterWait
        );
        assert_eq!(gate.last_emitted_text(), "Wait");
        assert_eq!(gate.last_candidate_text(), "Wait, don't");
        assert!(gate.has_pending_expansion());

        // 3. Progressive typewriter expansion 2
        assert_eq!(
            gate.evaluate_at("Wait, don't open", t0 + Duration::from_millis(100)),
            TextDiffAction::TypewriterWait
        );
        assert_eq!(gate.last_candidate_text(), "Wait, don't open");

        // 4. Progressive typewriter expansion 3 (final sentence text)
        assert_eq!(
            gate.evaluate_at(
                "Wait, don't open that door!",
                t0 + Duration::from_millis(150)
            ),
            TextDiffAction::TypewriterWait
        );
        assert_eq!(gate.last_candidate_text(), "Wait, don't open that door!");

        // 5. Duplicate candidate while waiting (game finished typing, screen idle)
        assert_eq!(
            gate.evaluate_at(
                "Wait, don't open that door!",
                t0 + Duration::from_millis(200)
            ),
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
        let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);

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

    #[test]
    fn test_text_gate_typewriter_with_spaced_tesseract_output() {
        use crate::ocr::normalize_cjk_text;
        let t0 = Instant::now();
        let mut gate = TextGate::new(Duration::from_millis(250), 0.92, 0.60);

        // Tesseract spacing differs between frames; normalisation makes them prefixes.
        let frames = ["私 は", "私は 学生", "私 は 学生 です 。"];
        let first = normalize_cjk_text(frames[0]);
        assert_eq!(
            gate.evaluate_at(&first, t0),
            TextDiffAction::EmitTranslation
        );
        for (i, raw) in frames.iter().enumerate().skip(1) {
            let text = normalize_cjk_text(raw);
            assert_eq!(
                gate.evaluate_at(&text, t0 + Duration::from_millis(50 * i as u64)),
                TextDiffAction::TypewriterWait,
                "frame {i} ({text}) should extend the previous line"
            );
        }
        assert_eq!(gate.flush_pending().as_deref(), Some("私は学生です。"));
    }
}
