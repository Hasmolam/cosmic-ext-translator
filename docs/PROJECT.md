# Project: cosmic-ext-translator Core & UX Enhancements

## Architecture
Multi-crate workspace under GPL-3.0-or-later:
- `crates/translator-protocol`: Shared types: HUD messages and socket, selector output, `Roi`, `CaptureRegion`, log setup.
- `crates/translator-core`: Core library (headless).
  - `capture/`: Screencast portal (`ashpd`), restore token negotiation, PipeWire OS loop, cropping and `ignore_roi` masking (`RoiPixels`).
  - `pipeline/`: Frame debouncer (`dhash_256`), high-pass blur subtraction, `preprocess.rs`, two-stage `TextGate`.
  - `ocr/`: `OcrBackend` trait, `layout.rs` (aspect ratio detection, vertical CJK topological sorter), `furigana.rs` (geometric ruby pruning), `OcrsBackend`, `TesseractBackend`, `MockOcrBackend`.
  - `translation/`: `TranslationBackend` trait, `context.rs` (sliding dialogue turns), `speaker.rs` (bracket parser & glossary), `rate_limit.rs` (HTTP 429 exponential backoff), remote backends.
  - `orchestrator/`: Async main loop coordinating debouncer, text gate, OCR, speaker parsing, translation, rate limiting, and IPC.
  - `ipc/`: Unix domain socket messages (`HudMessage`, status, subtitles).
- `crates/cosmic-translator-overlay`: Layer-shell overlay binary.
  - `--selector`: Interactive screen region & ignore area picker.
  - `--hud`: Click-through subtitle overlay (`input_zone: Some(vec![])`), non-destructive status badge.
- `crates/cosmic-applet-translator`: COSMIC panel applet.
  - `app/`: Application model and message routing; capture session, OCR backend, orchestrator, config change handling, popup and dashboard views.
  - `history.rs`: Backlog history model.
  - `settings/`: 5-tab settings (Capture, Engine, Display, Glossary, History), one module per tab.
  - `services/`: HUD client, overlay process launch, region selector.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Screencast Token Restoration & Fallback | Restore token negotiation in `ashpd` screencast, caching in applet config, fallback to interactive picker on token error, and clearing token on `CaptureFailed`. | M1 | ORIGINAL_REQUEST §R1 |
| 2 | Inner Ignore ROI Masking | Support `ignore_roi: Option<Roi>` on `CaptureRegion` and in `pipewire_thread.rs` buffer cropping to mask animated character bust/portrait regions before dHash & OCR. | M1 | ORIGINAL_REQUEST §R1 |
| 3 | Two-Stage Debouncer & TextGate | Two-stage hybrid debouncing: dHash frame stability + post-OCR `TextGate` evaluating Levenshtein similarity (jitter suppression) and monotonic prefix expansion (typewriter hold). | M1 | ORIGINAL_REQUEST §R1 |
| 4 | High-Pass Blur Subtraction Filter | Image preprocessing filter `subtract_blur` in `preprocess.rs` to attenuate smooth background gradients while preserving sharp text edges. | M1 | ORIGINAL_REQUEST §R1 |
| 5 | Vertical CJK Topological Sorter | Aspect ratio classification ($H/W > 1.4$), column clustering by horizontal projection overlap ($\ge 50\%$), right-to-left column ordering, and top-to-bottom character sequencing in `layout.rs`. | M2 | ORIGINAL_REQUEST §R1 |
| 6 | Geometric Furigana Pruning | Undersized ruby box detection ($< 35\%$ median area) adjacent to main text lines/columns with punctuation protection in `furigana.rs`. | M2 | ORIGINAL_REQUEST §R1 |
| 7 | Pluggable OCR Backend Architecture | `OcrBackend` trait enhancement with `recognize_detailed` returning `OcrResult { text, boxes, orientation }`, CJK joiner without ASCII spaces, and mockable CJK backends. | M2 | ORIGINAL_REQUEST §R1 |
| 8 | Multi-Turn Sliding Dialogue History | `TranslationContext` with `DialogueTurn { speaker, source, target, timestamp }`, configurable FIFO window, and speaker-attributed system prompt assembly for LLM backends. | M3 | ORIGINAL_REQUEST §R2 |
| 9 | Speaker Tag Parser & Glossary Resolution | Parsing bracketed speaker names (`【...】`, `〔...〕`, `[...]`, `Name:`) in `speaker.rs`, isolating dialogue body for translation, and resolving speaker name through character glossary. | M3 | ORIGINAL_REQUEST §R2 |
| 10 | Offline Translation Architecture | Removed after 0.2.0: the backend was a stub that never ran a model. | M3 | ORIGINAL_REQUEST §R2 |
| 11 | Morphological Dictionary Module | Removed after 0.2.0: not wired into the applet or orchestrator. | M3 | ORIGINAL_REQUEST §R2 |
| 12 | HTTP 429 Exponential Backoff Manager | `RateLimitManager` with backoff $30\text{s} \times 2^k$, pausing translation requests on HTTP 429 and ticking remaining seconds to HUD status display. | M3 | ORIGINAL_REQUEST §R2 |
| 13 | Dialogue Backlog History Viewer | `DialogueBacklog` storing up to 500 translation entries, draining `OrchestratorEvent` in `AppModel` to avoid channel deadlocks, and scrollable History tab in `settings/history.rs`. | M4 | ORIGINAL_REQUEST §R2 |
| 14 | HUD Status Badge & Overlay Countdown Feedback | Non-destructive status badge in `cosmic-translator-overlay` rendering active rate-limit countdown while preserving click-through input transparency (`input_zone: Some(vec![])`). | M4 | ORIGINAL_REQUEST §R2 |
| 15 | Applet UI Controls for Ignore ROI & Preprocessing | Settings UI controls for ignore area (draw/clear/readout) and blur subtraction toggle, plus selector `--ignore` mode in overlay. | M4 | ORIGINAL_REQUEST §R2 |
| 16 | Headless Quality Standards & Full Verification | Zero unwraps/panics in production code, strict clippy compliance, dedicated PipeWire thread, and 100% passing test suite headlessly with `PKG_CONFIG_PATH`. | M5 | ORIGINAL_REQUEST §R3 |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Capture & Debouncing Pipeline | Features 1, 2, 3, 4: Screencast restore token fallback, `ignore_roi` masking in `CaptureRegion`/`pipewire_thread.rs`, `TextGate` in `debouncer.rs`/`orchestrator`, and blur subtraction in `preprocess.rs`. | none | DONE |
| 2 | OCR & CJK Layout Analysis | Features 5, 6, 7: `layout.rs` topological sorter, `furigana.rs` ruby pruning, pluggable `OcrBackend` trait detailed results, CJK token joining without spaces. | none | DONE |
| 3 | Translation Context & Engines | Features 8, 9, 10, 11, 12: `context.rs` sliding dialogue turns, `speaker.rs` tag parser & glossary, and `rate_limit.rs` HTTP 429 backoff. | M1, M2 | PLANNED |
| 4 | Overlay & Applet User Experience | Features 13, 14, 15: Applet backlog history model, `OrchestratorEvent` channel drain in `app/translation.rs`, 5th History tab in settings dialog, HUD countdown badge in overlay, and ignore ROI/blur subtraction UI controls. | M1, M3 | PLANNED |
| 5 | E2E Integration & Final Quality Pass | Feature 16: Complete headless verification (`cargo check`, `cargo clippy -- -D warnings`, `cargo test --workspace`), 100% pass of E2E test suite (Tiers 1-4), adversarial test hardening (Tier 5), zero unwrap/expect in production code. | M1, M2, M3, M4 | PLANNED |

## Interface Contracts
### `crates/translator-protocol/src/region.rs`, `crates/translator-core/src/capture/crop.rs`
```rust
pub struct CaptureRegion {
    pub preset: RoiPreset,
    pub custom: Option<Roi>,
    pub ignore_roi: Option<Roi>,
}
pub trait RoiPixels { // translator-core
    fn mask_region(&self, image: &mut image::RgbaImage, ignore: &Roi);
}
```

### `crates/translator-core/src/pipeline/debouncer.rs`
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDiffAction {
    EmitTranslation,
    TypewriterWait,
    SuppressJitter,
}

pub struct TextGate { ... }
impl TextGate {
    pub fn evaluate(&mut self, candidate: &str) -> TextDiffAction;
}
```

### `crates/translator-core/src/pipeline/preprocess.rs`
```rust
pub struct PreprocessConfig {
    pub scale_factor: f32,
    pub binarize: bool,
    pub auto_invert: bool,
    pub blur_subtraction: bool,
    pub blur_sigma: f32,
}
pub fn subtract_blur(gray: &image::GrayImage, sigma: f32) -> image::GrayImage;
```

### `crates/translator-core/src/ocr/mod.rs` & `layout.rs` & `furigana.rs`
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextOrientation {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedTextBox {
    pub rect: Roi,
    pub text: String,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct OcrResult {
    pub text: String,
    pub boxes: Vec<DetectedTextBox>,
    pub orientation: TextOrientation,
}

pub fn sort_cjk_reading_order(boxes: Vec<DetectedTextBox>, orientation: TextOrientation) -> Vec<DetectedTextBox>;
pub fn filter_furigana(boxes: Vec<DetectedTextBox>, orientation: TextOrientation) -> Vec<DetectedTextBox>;
```

### `crates/translator-core/src/translation/`
```rust
pub struct ParsedDialogueLine {
    pub raw_speaker: Option<String>,
    pub resolved_speaker: Option<String>,
    pub dialogue: String,
    pub bracket_style: Option<BracketStyle>,
}
pub fn parse_speaker_tags(raw: &str, glossary: &HashMap<String, String>) -> ParsedDialogueLine;

pub struct DialogueTurn {
    pub speaker: Option<String>,
    pub source: String,
    pub target: String,
    pub timestamp_millis: u64,
}

pub struct RateLimitManager {
    pub fn record_rate_limit(&mut self) -> Duration;
    pub fn remaining_pause(&self) -> Option<Duration>;
    pub fn record_success(&mut self);
}
```

## Code Layout
- `crates/translator-protocol`:
  - `src/hud.rs`, `src/selector.rs`, `src/region.rs`, `src/logging.rs`
- `crates/translator-core`:
  - `src/capture/crop.rs`, `src/capture/pipewire_thread.rs`, `src/capture/screencast.rs`
  - `src/pipeline/debouncer.rs`, `src/pipeline/preprocess.rs`, `src/pipeline/mod.rs`
  - `src/ocr/layout.rs`, `src/ocr/furigana.rs`, `src/ocr/mod.rs`
  - `src/translation/context.rs`, `src/translation/speaker.rs`, `src/translation/rate_limit.rs`, `src/translation/mod.rs`
  - `src/orchestrator/mod.rs`
- `crates/cosmic-translator-overlay`:
  - `src/main.rs`, `src/hud.rs`, `src/selector.rs`
- `crates/cosmic-applet-translator`:
  - `src/config.rs`, `src/app/`, `src/settings/`, `src/services/`, `src/history.rs`

## Build & Test Verification Commands
All build and test commands must be executed with:
`PKG_CONFIG_PATH=/home/hasmolam/.local/lib/pkgconfig LIBRARY_PATH=/home/hasmolam/.local/lib LD_LIBRARY_PATH=/home/hasmolam/.local/lib`
- Verification check: `cargo check --workspace --all-targets`
- Clippy check: `cargo clippy --workspace --all-targets -- -D warnings`
- Test check: `cargo test --workspace`
