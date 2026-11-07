//! Detect when animated on-screen text has stopped changing.
//!
//! Games and visual novels draw dialogue one character at a time, fade it in, or
//! animate the background behind it. A tool that reads that text from screen captures
//! (OCR for translation, screen readers, subtitle extraction) should run once per line,
//! after the line is complete, and not on every frame.
//!
//! The crate has two stages:
//!
//! - [`FrameDebouncer`] works on pixels. It hashes each frame with a 256-bit difference
//!   hash and returns a frame once, after a change, when the region has been stable for
//!   a set time. Cheap enough to run on every captured frame.
//! - [`TextGate`] works on the recognised text. It drops recognition noise and repeats,
//!   and detects a line that is still being typed (each string extends the previous
//!   one), so only complete, new lines are passed on.
//!
//! # Example
//!
//! ```
//! use std::time::{Duration, Instant};
//! use image::{Rgba, RgbaImage};
//! use screen_text_settle::{FrameDebouncer, TextDiffAction, TextGate};
//!
//! // Frames of a dialogue box: empty, then one more white block per frame.
//! fn frame(blocks: u32) -> RgbaImage {
//!     let mut img = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
//!     for i in 0..blocks {
//!         for y in 10..40 {
//!             for x in i * 12..i * 12 + 8 {
//!                 img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
//!             }
//!         }
//!     }
//!     img
//! }
//!
//! let mut debouncer = FrameDebouncer::new(2, Duration::from_millis(200), Duration::from_secs(2));
//! let t0 = Instant::now();
//! assert!(debouncer.process_frame_at(frame(0), t0).is_none()); // baseline
//! assert!(debouncer.process_frame_at(frame(1), t0 + Duration::from_millis(30)).is_none());
//! assert!(debouncer.process_frame_at(frame(2), t0 + Duration::from_millis(60)).is_none());
//! // No change for 200 ms: the last frame is returned once.
//! let settled = debouncer.process_frame_at(frame(2), t0 + Duration::from_millis(300));
//! assert!(settled.is_some());
//!
//! // Text recognised from successive frames.
//! let mut gate = TextGate::default();
//! assert_eq!(gate.evaluate("Wait"), TextDiffAction::EmitTranslation);
//! assert_eq!(gate.evaluate("Wait, don't open"), TextDiffAction::TypewriterWait);
//! assert_eq!(gate.evaluate("Wait, don't open that door!"), TextDiffAction::TypewriterWait);
//! assert_eq!(gate.flush_pending().as_deref(), Some("Wait, don't open that door!"));
//! assert_eq!(gate.evaluate("Wait, don't open that door"), TextDiffAction::SuppressJitter);
//! ```
//!
//! Neither type keeps a timer. [`FrameDebouncer`] only checks the time when a frame
//! arrives; if the capture source sends frames only on change, call
//! [`FrameDebouncer::force_settle`] from a timer. After
//! [`TextDiffAction::TypewriterWait`], wait for the line to finish and call
//! [`TextGate::flush_pending`].

pub mod frame;
pub mod hash;
pub mod text;

pub use frame::{DebounceState, FrameDebouncer, FrameImage, SettledFrame};
pub use hash::{dhash_256, dhash_64, hamming_distance_256, hamming_distance_64};
pub use text::{TextDiffAction, TextGate};
