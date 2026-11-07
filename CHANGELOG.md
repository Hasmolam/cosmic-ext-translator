# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Release notes on GitHub are generated from
the section of the released version.

## [Unreleased]

### Added

- Log files in `~/.local/state/cosmic-ext-translator/`, one per process. Processes
  started from the panel or app launcher had no readable log before.
- The status shows "Starting…" while the screen share dialog is open, and the reason
  when capture cannot be started. Cancelling the dialog is not reported as an error.

### Changed

- New crate `screen-text-settle` (`MIT OR Apache-2.0`) with the frame debouncer and
  the text gate, prepared for crates.io. `TextGate::new` takes only the jitter
  threshold: the typewriter duration and turn threshold were stored but never read.
  The fields of `TextGate` and `FrameDebouncer` are private, with getters.
- New crate `cjk-ocr-layout` (`MIT OR Apache-2.0`) with orientation detection,
  vertical reading order, the furigana filter and CJK text normalisation, prepared for
  crates.io. It has its own `Rect` type; `DetectedTextBox` is renamed to `TextBox`.
  serde support is behind the `serde` feature.
- CI runs `cargo publish --dry-run`, a build of the packaged crate with its minimum
  Rust version and `cargo semver-checks` for both library crates.
- `target/` is limited to `TARGET_LIMIT_GB` (default 30) by the `just` build, check
  and test recipes: incremental caches are removed first, then `cargo clean` runs.
  Debug builds keep only line tables for workspace crates and no debug info for
  dependencies. `target/` had grown to 129 GB; after a clean clippy and test run it
  is 4.4 GB.
- New crate `translator-protocol` with the HUD messages and socket, selector output,
  regions and log setup. The overlay depends only on it instead of `translator-core`:
  clean release build 143 s → 112 s, binary 39.3 MB → 37.6 MB.
- New crate `translator-translate` with the translation engines, dialogue context,
  speaker tags, fuzzy cache and rate limiter. `translator-core` re-exports it as
  `translator_core::translation`, so existing paths keep working.
- New crate `translator-capture` with the screen-sharing portal, the PipeWire capture
  thread, region cropping and `CaptureError`. Re-exported as `translator_core::capture`.
- New crate `translator-ocr` with the OCR engines, model download, CJK layout and
  furigana filter. Re-exported as `translator_core::ocr`. Language codes moved to
  `translator_protocol::language`.
- `just install-user` builds the release binaries first and restarts only the panel
  applet (SIGHUP, exit code 75, restarted by cosmic-panel) instead of killing
  cosmic-panel. Repeated installs killed the panel often enough for cosmic-session to
  delay its restart by minutes and leave it without the notifications connection.
  `just uninstall-user` stops the applet and HUD instead of the panel.
- The target language is chosen from a list of 15 languages that Google, DeepL and LLM
  engines all accept, instead of typing a code. A code outside the list that was set
  earlier is kept and shown as a separate entry.
- Applet code split into `app/` and `settings/` modules, one settings module per tab.
  Config changes are applied from the difference between old and new settings, so
  saving a setting no longer restarts translation twice, and changing a setting no
  longer starts the HUD.
- Glossary terms are listed in alphabetical order instead of hash order.

### Fixed

- Log files contained terminal colour codes in span fields.
- HUD and settings window processes started by the applet stayed as zombies after
  they exited until the applet itself was restarted.
- Region selector left trails of earlier rectangles while dragging. The software
  renderer did not repaint the outer half of the selection border; the redrawn area
  now includes a margin around it.
- HUD calibration: "Up" and "Down" moved the HUD in the opposite direction when it was
  anchored to the bottom edge, and did nothing when it was centered. The buttons now
  follow the anchor and are hidden for the centered position.
- HUD calibration was lost on the next style update. Locking the position now stores
  position and offset in the applet settings.
- The live inspection preview in the settings window was only filled when capture
  started, usually before the first frame arrived. It now follows the capture stream at
  up to 4 updates per second.

### Removed

- `translator_core::dictionary` (`JapaneseDictionary`, `MorphemeCard`) and
  `OfflineTranslationBackend`. Neither was reachable from the applet; the offline
  backend returned placeholder text without running a model.

### Changed

- Integration tests are grouped by component (`text_gate`, `ocr_layout`, `furigana`,
  `rate_limit`, `hud_ipc`, ...) instead of by development milestone.

## [0.2.0] - 2026-11-06

### Added

- Japanese and Simplified Chinese OCR with PaddleOCR PP-OCRv5 models, downloaded on first
  use from the `models-v1` release and verified with SHA-256.
- Tesseract backend for Japanese, Chinese and Korean, including vertical text
  (`jpn_vert`) and automatic horizontal/vertical selection.
- Source language, text direction and OCR engine settings in the Engine tab.
- Furigana removal and CJK text normalisation before translation.
- OCR status in the applet popup and HUD when no OCR engine can be loaded.
- Release assets are built by GitHub Actions with a `SHA256SUMS` file and build
  provenance attestations.

### Changed

- Application ID changed from `com.system76.CosmicExtTranslator` to
  `io.github.hasmolam.CosmicExtTranslator`. Settings are copied to the new location on
  first start; `install.sh` and `just install-user` also update the panel entry. After a
  `.deb` or Arch package upgrade, add the applet to the panel again.
- `install.sh` verifies the downloaded archive against `SHA256SUMS`, installs the desktop
  entry from the archive, and accepts `--version vX.Y.Z`.
- OCR models are downloaded as ONNX files; the former `.rten` URLs return HTTP 403.
  Existing `.rten` files are still used.
- `Cargo.lock` is committed and release builds use `--locked`.

### Fixed

- Subtitle position, vertical offset and positioning mode changes now apply to the running
  overlay. They were sent as a second layer-surface creation, which the toolkit ignores, so
  they only took effect after the overlay restarted.
- Interrupted or corrupt model downloads no longer leave OCR permanently broken.
- Desktop entry and AppStream metadata now pass `desktop-file-validate` and
  `appstreamcli validate`.

## [0.1.0] - 2026-11-03

Initial release.

[Unreleased]: https://github.com/Hasmolam/cosmic-ext-translator/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Hasmolam/cosmic-ext-translator/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Hasmolam/cosmic-ext-translator/releases/tag/v0.1.0
