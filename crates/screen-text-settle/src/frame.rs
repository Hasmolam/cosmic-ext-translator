//! Frame-level settle detection.

use image::RgbaImage;
use std::time::{Duration, Instant};
use tracing::{debug, trace};

use crate::hash::{dhash_256, hamming_distance_256};

/// A frame the debouncer can hash.
///
/// Implemented for [`RgbaImage`]. Implement it for capture types that carry metadata
/// (timestamps, source size) next to the pixels, so the settled frame is returned with
/// that metadata intact.
pub trait FrameImage {
    /// Pixels to hash.
    fn image(&self) -> &RgbaImage;
}

impl FrameImage for RgbaImage {
    fn image(&self) -> &RgbaImage {
        self
    }
}

/// State of a [`FrameDebouncer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebounceState {
    /// Screen is stable, no active text writing detected.
    Idle,
    /// pHash differences detected; dialogue is streaming/typing.
    Animating,
    /// Dialogue text has finished typing and stabilized.
    Settled,
}

/// A frame that stayed visually stable after a change.
#[derive(Debug, Clone)]
pub struct SettledFrame<F> {
    /// The last frame received before the region settled.
    pub frame: F,
    /// The 256-bit dHash of the frame folded to 64 bits with XOR. Equal frames have
    /// equal values; use it to skip work on a frame that was already processed.
    pub hash: u64,
    /// Time the region was declared settled.
    pub settled_at: Instant,
}

/// Detects when animated on-screen content stops changing.
///
/// Each frame is reduced to a 256-bit difference hash. A Hamming distance of at least
/// `hash_threshold` from the previous frame starts or extends an animation. When no
/// such change has been seen for `settle_duration`, or the animation has lasted
/// `max_animation_duration`, the latest frame is returned once as a [`SettledFrame`].
#[derive(Debug)]
pub struct FrameDebouncer<F> {
    hash_threshold: u32,
    settle_duration: Duration,
    max_animation_duration: Duration,

    state: DebounceState,
    last_hash: Option<[u64; 4]>,
    last_change_time: Instant,
    animation_start_time: Option<Instant>,
    latest_frame: Option<F>,
}

/// Threshold 3, settle 180 ms, maximum animation 1.5 s.
impl<F: FrameImage> Default for FrameDebouncer<F> {
    fn default() -> Self {
        Self::new(3, Duration::from_millis(180), Duration::from_millis(1500))
    }
}

impl<F: FrameImage> FrameDebouncer<F> {
    /// `hash_threshold`: Hamming distance (out of 256 bits) at which a frame counts as
    /// changed. Lower values react to smaller changes and to more noise.
    ///
    /// `settle_duration`: time without a change before the frame is returned.
    ///
    /// `max_animation_duration`: upper bound on the wait, for content that never stops
    /// moving, such as a blinking cursor next to the text.
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

    /// Current state.
    #[must_use]
    pub fn state(&self) -> DebounceState {
        self.state
    }

    /// Hamming distance at which a frame counts as changed.
    #[must_use]
    pub fn hash_threshold(&self) -> u32 {
        self.hash_threshold
    }

    /// Time without a change before a frame is returned.
    #[must_use]
    pub fn settle_duration(&self) -> Duration {
        self.settle_duration
    }

    /// Upper bound on the wait during a continuous animation.
    #[must_use]
    pub fn max_animation_duration(&self) -> Duration {
        self.max_animation_duration
    }

    /// Processes a frame received now.
    ///
    /// Returns the frame once, after a change, when nothing has changed for
    /// `settle_duration`. The first frame only sets the baseline.
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

                    // Back to Idle for the next change.
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

    /// Returns the pending frame if an animation is in progress.
    ///
    /// Screen capture often delivers frames only when the content changes, so after the
    /// last change no further frame may arrive to complete the settle check. Call this
    /// from a timer `settle_duration` after the last frame.
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

    /// Forgets the previous frame, for example after the captured region changed.
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
}
