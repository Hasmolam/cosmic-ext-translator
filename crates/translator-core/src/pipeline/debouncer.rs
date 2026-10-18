use std::time::{Duration, Instant};
use tracing::{debug, trace};

use crate::capture::pipewire_thread::CapturedFrame;
use crate::pipeline::phash::{dhash_256, hamming_distance_256};

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
pub struct SettledFrame {
    pub frame: CapturedFrame,
    pub hash: u64,
    pub settled_at: Instant,
}

pub struct FrameDebouncer {
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
    latest_frame: Option<CapturedFrame>,
}

impl Default for FrameDebouncer {
    fn default() -> Self {
        Self::new(3, Duration::from_millis(180), Duration::from_millis(1500))
    }
}

impl FrameDebouncer {
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

    /// Ingests a new captured frame from PipeWire.
    ///
    /// Returns `Some(SettledFrame)` only when the text stream has completed typing
    /// and remained visually stable for `settle_duration`.
    pub fn process_frame(&mut self, frame: CapturedFrame) -> Option<SettledFrame> {
        let current_hash = dhash_256(&frame.image);
        let now = Instant::now();

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

                // Only treat changes with distance >= 3 as active text writing.
                // 1-2 bit fluctuations are subpixel rasterization jitter or idle background sprite breathing.
                if distance >= 3 {
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
                    let compact_hash = current_hash[0] ^ current_hash[1] ^ current_hash[2] ^ current_hash[3];
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

    /// Resets internal state (useful when changing ROI or switching game windows).
    pub fn reset(&mut self) {
        self.state = DebounceState::Idle;
        self.last_hash = None;
        self.latest_frame = None;
        self.animation_start_time = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn make_frame_with_blocks(num_blocks: u32) -> CapturedFrame {
        let mut img = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
        for i in 0..num_blocks {
            let start_x = (i * 12).min(90);
            for y in 10..40 {
                for x in start_x..(start_x + 8).min(100) {
                    img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
                }
            }
        }
        CapturedFrame {
            image: img,
            timestamp: Instant::now(),
            original_width: 100,
            original_height: 50,
        }
    }

    #[test]
    fn test_debouncer_typewriter_simulation() {
        let mut debouncer = FrameDebouncer::new(
            2,
            Duration::from_millis(50),
            Duration::from_millis(500),
        );

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
        assert!(settled.is_some(), "Debouncer should emit settled frame after stability window");
        assert_eq!(debouncer.state(), DebounceState::Idle);
    }
}
