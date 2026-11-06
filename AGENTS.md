# cosmic-ext-translator Developer Guidelines

## Workspace Structure & Entrypoints

Multi-crate workspace under GPL-3.0-or-later:

- `crates/translator-core`: Core library (no GUI). Handles Screencast portal (`ashpd`), dedicated PipeWire loop (`pipewire`), screen regions (`region.rs`), ROI crop (`capture/crop.rs`), dHash/typewriter debouncing (`debouncer.rs`), image pre-processing (`preprocess.rs`), pure-Rust OCR (`ocrs` + `rten`), translation engines (Google, Ollama), fuzzy LRU cache (`strsim`), IPC socket server/client, and orchestrator loop.
- `crates/cosmic-translator-overlay`: Layer-shell overlay binary (`cosmic-translator-overlay`).
  - `--selector`: Interactive screen region picker (`Layer::Overlay`, `KeyboardInteractivity::Exclusive`). Emits `ROI_SELECTED:{"x":...,"y":...,"width":...,"height":...}` to stdout on release and exits 0.
  - `--hud`: Click-through subtitle overlay (`Layer::Overlay`, `input_zone: Some(vec![])`). Listens on Unix domain socket for `HudMessage` payloads.
- `crates/cosmic-applet-translator`: COSMIC panel applet (`io.github.hasmolam.CosmicExtTranslator`, 0.1.x: `com.system76.CosmicExtTranslator`, migrated in `config.rs`). Owns applet popup, 5-tab settings window, `cosmic-config` persistence, and global shortcuts subscription.

## Development & Verification Commands

Validation pipeline:

```sh
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Focused test execution:

```sh
# Run single unit test
cargo test -p translator-core <test_name>

# Run integration capture tool (requires active Wayland + PipeWire session)
cargo run -p translator-core --example test_capture

# Run applet in dev mode
cargo run -p cosmic-applet-translator

# Run overlay in selector or hud mode manually
cargo run -p cosmic-translator-overlay -- --selector
cargo run -p cosmic-translator-overlay -- --hud
```

Installation and packaging:

```sh
just build-release
sudo just install
```

## Architectural Rules & Gotchas

1. **PipeWire Buffer Lifecycles:**
   - In `pipewire_thread.rs`, `stream.dequeue_buffer()` must only crop the selected ROI into an allocated `RgbaImage` and drop the buffer immediately. Never hold or send raw PipeWire buffers across async tasks.
   - PipeWire event loop runs on a dedicated OS thread (`std::thread`), not inside the Tokio async runtime.

2. **Layer Shell & Input Transparency:**
   - The HUD subtitle window MUST keep `input_zone: Some(vec![])` to guarantee 100% click-through to underlying full-screen games.
   - Do not request keyboard interactivity or set non-empty input zones on the HUD surface.

3. **IPC Protocol:**
   - Applet communicates with HUD via Unix domain socket at `$XDG_RUNTIME_DIR/cosmic-ext-translator-hud.sock` (fallback: `/tmp/cosmic-ext-translator-hud.sock`).
   - If HUD binary is not running when translation starts, `cosmic-applet-translator` auto-spawns `cosmic-translator-overlay --hud`.

4. **OCR & Dev Profile Speed:**
   - Dev builds will be extremely slow at OCR inference unless optimization is enabled. Workspace `Cargo.toml` overrides `ocrs`, `rten`, and `rten-vecmath` with `opt-level = 3` in dev profile. Keep this intact.
   - Models are stored in `$XDG_DATA_HOME/cosmic-ext-translator/models`. Tests must use `MockOcrBackend` to avoid mandatory network access during CI.

5. **Typewriter Debouncing:**
   - Dialogue streams in RPGs/VNs letter-by-letter. `FrameDebouncer` uses 64-bit dHash and Hamming distance.
   - Only frames that remain stable for `settle_duration` (default 250ms) transition from `Animating` to `Settled` and trigger OCR.

6. **Rust Quality Standards:**
   - Production code must not call `.unwrap()`, `.expect()`, or `panic!()`. Return typed `thiserror` / `anyhow` errors.
   - Log errors and events through `tracing::{debug, info, warn, error}`.
   - Keep codebase clean of clippy warnings (`-D warnings`).

7. **Tone and Style:**
   - No marketing buzzwords, hype, emojis, sycophancy, or fluff.
   - Keep documentation, comments, commit messages, and summaries direct, technical, and objective.
