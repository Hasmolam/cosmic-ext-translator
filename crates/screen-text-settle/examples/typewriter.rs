//! Feeds a simulated typewriter dialogue box through both stages and prints what a
//! caller would act on.
//!
//! `cargo run -p screen-text-settle --example typewriter`

use std::time::{Duration, Instant};

use image::{Rgba, RgbaImage};
use screen_text_settle::{FrameDebouncer, TextDiffAction, TextGate};

/// A dialogue box with `chars` characters drawn as white blocks.
fn dialogue_box(chars: usize) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(240, 40, Rgba([20, 20, 40, 255]));
    for c in 0..chars as u32 {
        let x0 = 4 + (c % 29) * 8;
        let y0 = 6 + (c / 29) * 16;
        for y in y0..y0 + 10 {
            for x in x0..x0 + 5 {
                img.put_pixel(x, y, Rgba([240, 240, 240, 255]));
            }
        }
    }
    img
}

fn main() {
    let line = "Wait, don't open that door!";
    let mut debouncer = FrameDebouncer::new(2, Duration::from_millis(200), Duration::from_secs(3));
    let mut gate = TextGate::default();

    let start = Instant::now();
    let frame_interval = Duration::from_millis(33);
    // One character every two frames, a pause after the comma (as many games do), then
    // a second of the finished line.
    let comma = 5;
    let frames = (0..=comma * 2)
        .map(|i| i / 2)
        .chain(std::iter::repeat_n(comma, 10))
        .chain((comma * 2..=line.len() * 2).map(|i| i / 2))
        .chain(std::iter::repeat_n(line.len(), 30));

    for (i, chars) in frames.enumerate() {
        let now = start + frame_interval * i as u32;
        let Some(settled) = debouncer.process_frame_at(dialogue_box(chars), now) else {
            continue;
        };
        let ms = settled.settled_at.duration_since(start).as_millis();
        // An OCR engine would read `settled.frame` here; use the known text instead.
        let text = &line[..chars];
        match gate.evaluate(text) {
            TextDiffAction::EmitTranslation => println!("{ms:>5} ms  new line: {text}"),
            TextDiffAction::TypewriterWait => println!("{ms:>5} ms  still typing: {text}"),
            TextDiffAction::SuppressJitter => println!("{ms:>5} ms  ignored: {text}"),
        }
    }
    // The line stopped growing: take the complete text.
    if let Some(text) = gate.flush_pending() {
        println!("          complete line: {text}");
    }
}
