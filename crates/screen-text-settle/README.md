# screen-text-settle

Detect when animated on-screen text has stopped changing, then emit it once.

Games and visual novels draw dialogue one character at a time, fade it in, or animate
the background behind it. A tool that reads that text from screen captures (OCR for
translation, screen readers, subtitle extraction) should run once per line, after the
line is complete, and not on every frame.

- `FrameDebouncer` hashes each frame with a 256-bit difference hash and returns a frame
  once, after a change, when the region has been stable for a set time.
- `TextGate` takes the recognised text and drops recognition noise and repeats. It
  reports a line that is still being typed, where each string extends the previous one.

The crate does not capture the screen or run OCR. It takes `image::RgbaImage` frames,
or any type that implements `FrameImage`, and strings.

## Usage

```toml
[dependencies]
screen-text-settle = "0.1"
```

```rust
use std::time::Duration;
use screen_text_settle::{FrameDebouncer, TextDiffAction, TextGate};

let mut debouncer = FrameDebouncer::new(2, Duration::from_millis(200), Duration::from_secs(2));
let mut gate = TextGate::default();

for frame in capture_frames() {
    let Some(settled) = debouncer.process_frame(frame) else { continue };
    let text = run_ocr(&settled.frame);
    match gate.evaluate(&text) {
        TextDiffAction::EmitTranslation => handle_line(&text),
        TextDiffAction::TypewriterWait => { /* flush_pending() after a delay */ }
        TextDiffAction::SuppressJitter => {}
    }
}
```

`examples/typewriter.rs` runs both stages over a simulated dialogue box:

```sh
cargo run -p screen-text-settle --example typewriter
```

```text
  561 ms  new line: Wait,
 2310 ms  still typing: Wait, don't open that door!
          complete line: Wait, don't open that door!
```

## Tuning

| Parameter | Default | Effect |
|---|---|---|
| `hash_threshold` | 3 | Bits out of 256 that must differ for a frame to count as changed. Lower reacts to smaller glyphs and to more noise. |
| `settle_duration` | 180 ms | Time without a change before the frame is returned. Longer than the gap between two typed characters. |
| `max_animation_duration` | 1.5 s | Upper bound for content that never stops moving, such as a blinking cursor. |
| `jitter_threshold` | 0.92 | Normalised Levenshtein similarity at which a string counts as the previous one with recognition noise. |

Neither type keeps a timer. If the capture source sends frames only when the content
changes, call `FrameDebouncer::force_settle` from a timer `settle_duration` after the
last frame.

## Origin

Extracted from [cosmic-ext-translator](https://github.com/Hasmolam/cosmic-ext-translator),
a screen translator for the COSMIC desktop.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.
