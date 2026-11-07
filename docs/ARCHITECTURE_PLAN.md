# Architecture Review and Restructuring Plan

Status: draft, 2026-11-07. Scope: workspace layout, crate boundaries, applet code
structure, and which parts are worth publishing on crates.io. No behaviour changes are
planned here; feature work continues in [JAPANESE_SUPPORT_PLAN.md](JAPANESE_SUPPORT_PLAN.md)
and [COMPARATIVE_ANALYSIS_AND_ROADMAP.md](COMPARATIVE_ANALYSIS_AND_ROADMAP.md).

## 1. Current State

Three crates, 19.4k lines of Rust including tests (v0.2.0):

| Crate | Role | Notable size |
|---|---|---|
| `translator-core` | capture, pipeline, OCR, translation, IPC, orchestrator, shortcuts, dictionary | `ocr/tesseract.rs` 596, `ocr/mod.rs` 586, `capture/roi.rs` 577, `pipeline/debouncer.rs` 575, `ocr/paddle.rs` 574 |
| `cosmic-applet-translator` | panel applet, popup, settings window, config | `app.rs` 1880, `settings_dialog.rs` 694 |
| `cosmic-translator-overlay` | HUD and region selector | `main.rs` 405 |

Release binaries: applet 62 MB, overlay 39 MB.

### 1.1 Reference point

File length alone is not the problem. Upstream COSMIC applications keep large single
files: `cosmic-files/src/app.rs` is about 300 kB, and `cosmic-term`, `cosmic-edit` and
`cosmic-store` each have a 120-160 kB `main.rs`. Those projects have full-time
maintainers who know the file. For a project that wants outside contributors, the costs
are review difficulty, merge conflicts on one hot file, and slow incremental builds when
one crate holds everything. `cosmic-settings` is the better model for the settings
window: one module per page (`pages/display`, `pages/sound`, ...) with its own
`Message`, `update` and `view`.

## 2. Problems

Ordered by cost to development.

1. **`app.rs` `update()` is about 1000 lines (lines 644-1651).** It handles 30 `Message`
   variants and 38 `SettingsMessage` arms inline. Side effects (process spawning, file
   I/O, socket clients) are written inside the match arms.
2. **The region selector flow is duplicated.** `SelectRoiRequested` (around line 1176)
   and `SelectIgnoreRoiRequested` (around line 1286) each write a preview BMP, spawn
   `cosmic-translator-overlay --selector`, parse `ROI_SELECTED:` lines, and declare
   their own local `SelectedPayload` struct. The stdout format is a second IPC protocol
   that is not defined in a shared place.
3. **`HudIpcClient::default_client()` is constructed at 15 call sites** in `app.rs`.
   HUD auto-spawn is in a separate code path (line 547). There is no single owner of
   the HUD connection.
4. **The overlay links all of `translator-core`.** It uses only `ipc` types and `Roi`,
   but compiles and links PipeWire, ashpd, ocrs, rten, reqwest and the model downloader.
   This shows up in both build time and binary size.
5. **Wrong dependency direction inside `translator-core`.**
   - `ocr/*` imports `capture::roi::Roi` only for rectangle geometry.
   - `pipeline/debouncer.rs` takes `capture::pipewire_thread::CapturedFrame`, a type
     defined next to PipeWire code, even though it only needs an `RgbaImage` and a
     timestamp. This blocks extracting the debouncer as a standalone crate.
6. **The public API is flat.** `lib.rs` re-exports about 60 items, including mocks
   (`MockOcrBackend`, `MockTranslationBackend`). Every type is public API. That is
   harmless now but makes any published crate break semver on every refactor.
7. **Dead code.** `dictionary` (`JapaneseDictionary`, `MorphemeCard`) and
   `translation/offline.rs` (`OfflineTranslationBackend`) are only reached from
   `lib.rs` re-exports and `tests/adversarial_m3.rs`.
8. **Tests are named by milestone** (`adversarial_m1.rs` ... `adversarial_m4_challenge.rs`,
   `adversarial_m3.rs` is 1409 lines). A contributor changing the debouncer cannot tell
   which file covers it.

## 3. Target Layout

Split along heavy-dependency boundaries, not per file. Each crate must either remove a
heavy dependency from some consumer or be a candidate for publication. Small modules
stay modules.

```text
crates/
  screen-text-settle      lib  image                       publishable
  cjk-ocr-layout          lib  no heavy deps               publishable
  translator-protocol     lib  serde (+ tokio, feature)    publish = false
  translator-capture      lib  ashpd, pipewire             publish = false
  translator-ocr          lib  rten, ocrs (features)       publish = false
  translator-translate    lib  reqwest, strsim             publish = false
  translator-core         lib  orchestrator + pipeline     publish = false
  cosmic-applet-translator  bin
  cosmic-translator-overlay bin  depends only on translator-protocol
```

Dependency graph (arrows point to dependencies):

```text
applet --> core --> capture  --> protocol
               \--> ocr      --> cjk-ocr-layout, protocol
               \--> translate
               \--> screen-text-settle
overlay --> protocol
```

Crate contents:

| Crate | Moves from | Notes |
|---|---|---|
| `screen-text-settle` | `pipeline/debouncer.rs`, `pipeline/phash.rs` | `FrameDebouncer`, `TextGate`, dHash. Input becomes `(&RgbaImage, Instant)`. Uses a `normalize` callback or a small built-in normaliser instead of importing `ocr::normalize_cjk_text`. |
| `cjk-ocr-layout` | `ocr/layout.rs`, `ocr/furigana.rs`, `ocr/normalize.rs`, `join_cjk_or_latin_tokens` | Defines its own `Rect`/`TextBox`. No OCR engine dependency; works on boxes from any engine. |
| `translator-protocol` | `ipc/mod.rs`, `Roi` and `RoiPreset` from `capture/roi.rs`, selector output | One definition of `HudMessage`, `HudPosition`, `HudStatus`, socket path, and a typed `SelectorOutput` replacing the ad-hoc `ROI_SELECTED:` parsing. Async client/server behind a `tokio` feature. |
| `translator-capture` | `capture/` (minus `Roi`), `shortcuts/` | Owns the PipeWire thread and portals. |
| `translator-ocr` | `ocr/` (minus layout code), `language.rs`, `OcrInitError` | Features: `paddle` (rten + model download), `ocrs`, `tesseract` (subprocess, no heavy deps). Default: all on. |
| `translator-translate` | `translation/` | Backends, fuzzy cache, rate limiter, speaker tags. |
| `translator-core` | `pipeline/mod.rs`, `orchestrator/` | Wires the crates together. Re-exports only what the applet uses. |

Rules that go with the layout:

- Virtual workspace manifest stays at the root, crates stay flat under `crates/`, crate
  name equals directory name.
- All dependency versions in `[workspace.dependencies]`; member crates use
  `workspace = true`.
- Internal crates set `publish = false`. Only the two library crates are published.
- Test doubles (`MockOcrBackend`, `MockTranslationBackend`) move behind a `test-util`
  feature instead of being re-exported unconditionally.
- Public enums and config structs that will grow get `#[non_exhaustive]`.
- Keep the `[profile.dev.package.*] opt-level = 3` overrides for `ocrs`, `rten` and
  `rten-vecmath`.

## 4. Applet Structure

Follow the Elm structure that libcosmic expects, with nested update delegation as in
`cosmic-settings`:

```text
src/
  main.rs
  config.rs
  i18n.rs
  app/
    mod.rs          AppModel, top-level Message, routing, subscriptions (target < 400 lines)
    capture.rs      capture session lifecycle, ROI and ignore-ROI messages
    ocr.rs          OCR backend loading state (OcrStatus)
    popup.rs        popup view
    dashboard.rs    standalone dashboard view
    history.rs      (existing)
  settings/
    mod.rs          window, tab routing
    general.rs  capture.rs  ocr.rs  translation.rs  hud.rs
                    each: enum Message, fn update(&mut Config, Message) -> Task<Message>, fn view
  services/
    hud.rs          HudClient: single owner of the socket client and HUD auto-spawn
    selector.rs     async fn select_region(mode, preview) -> Result<Option<SelectorOutput>>
```

Conventions:

- `update` arms call a function and return its `Task`. Process spawning, file I/O and
  socket calls live in `services/` and are tested without a GUI.
- Child modules return `Task<child::Message>`; the parent maps it with
  `.map(Message::Child)`.
- The ROI and ignore-ROI flows share `services::selector::select_region`, which removes
  the duplicated block and the two local `SelectedPayload` structs.

## 5. Publishing on crates.io

Assessment of each candidate against the current crates.io landscape (checked
2026-11-07):

| Candidate | Existing alternatives | Verdict |
|---|---|---|
| Typewriter settle detector (`screen-text-settle`) | No crate covers "wait until animated on-screen text stops changing, then emit once". | Publish. Clear niche: VN/RPG translators, game accessibility tools, screen readers, streaming tools. |
| CJK OCR post-processing (`cjk-ocr-layout`) | No crate found for furigana pruning or vertical reading order on OCR boxes. | Publish. Engine-agnostic, so users of `oar-ocr`, `paddle-ocr-rs` or `ocrs` can use it. |
| PP-OCRv5 on rten | `oar-ocr` (345k downloads), `ocr-rs` (86k), `paddle-ocr-rs` (26k), `pure-onnx-ocr` (pure Rust, PP-OCRv5/v6). | Do not publish as a separate crate. Low differentiation. Consider offering the CTC/dictionary fix or an example upstream to `rten`/`ocrs` instead. |
| Translation backends | Several API-client crates exist. | Do not publish. App-specific. |
| HUD protocol | App-specific. | `publish = false`. |

All names above were free on crates.io on 2026-11-07, as were `typewriter-settle`,
`furigana-filter` and `cjk-reading-order`.

Requirements before the first `cargo publish`:

- `README.md`, crate-level docs with a runnable example, `examples/`, and docs.rs
  metadata in each published crate.
- `rust-version` set, and `cargo publish --dry-run` plus `cargo semver-checks` in CI
  for the two published crates.
- Path dependencies between published crates use both `path` and `version`; crates.io
  rejects dependencies without a registry version.
- License decision (see below).

### 5.1 License

The workspace is `GPL-3.0-or-later`. A GPL library crate cannot be used by MIT/Apache
projects, which are the large majority of the Rust ecosystem. Since all code has a single
author, the two published crates can be relicensed as `MIT OR Apache-2.0` (the Rust
ecosystem convention) or `MPL-2.0` (file-level copyleft), while the applications and the
internal crates stay GPL. This is the maintainer's decision; with GPL the published
crates are unlikely to see use outside GPL projects.

## 6. Adoption

Library crates produce downloads and occasional issues. Stars mostly come from the
application, from people who play games or read visual novels. The items with the most
effect, in order:

1. A short demo video or GIF at the top of the README: a Japanese game line being
   replaced by its translation in the HUD.
2. Distribution through the channels COSMIC users already use: Flathub (shown in COSMIC
   Store) and the AUR. Flatpak packaging needs verification for the overlay subprocess,
   the screencast portal and the Tesseract fallback inside the sandbox.
3. Posts with the demo in communities that need the tool: r/visualnovels,
   r/LearnJapanese, r/pop_os, r/rust (the latter for the library crates).
4. Listing in community indexes of COSMIC applets; consider moving the repository to
   or listing it under the `cosmic-utils` organisation if its maintainers accept it.
5. `good first issue` labels on isolated tasks. The restructuring in section 3 is what
   makes such tasks possible: a settings page or a translation backend becomes one file
   with its own tests.

## 7. Migration Phases

Each step is one commit, keeps behaviour unchanged, and passes
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test --workspace`. Record before and after numbers for clean build time,
incremental build after touching `app.rs`, test time, and binary sizes.

**A0. Cleanup.**
- Remove `dictionary/` and `translation/offline.rs` with their re-exports and tests,
  or move them behind a feature if they are planned work.
- Rename integration tests by component (`debouncer.rs`, `ocr_layout.rs`,
  `translation_cache.rs`, `hud_ipc.rs`, ...). Split `adversarial_m3.rs` by component.

**A1. Fix dependency direction inside `translator-core`.** Done.
- `FrameDebouncer<F: FrameImage>` is generic over the frame type; `RgbaImage` and
  `CapturedFrame` implement `FrameImage`. `process_frame_at` takes an explicit
  `Instant`. The debouncer no longer imports `capture`.
- `Roi`, `RoiPreset` and `CaptureRegion` moved to `region.rs` (serde only). Pixel
  operations (`crop_to_rgba`, `mask_region`) stay in `capture/crop.rs`. `ocr` no longer
  imports `capture`.
- `capture` now imports `pipeline::debouncer::FrameImage`. After extraction this
  becomes a dependency of `translator-capture` on `screen-text-settle`, which only pulls
  in `image`.
- A dedicated rectangle type for the layout code is deferred to A5, where it is part of
  the `cjk-ocr-layout` public API. While `Roi` is in a dependency-free module it adds
  churn without removing a dependency.

**A2. Applet services.** Done.
- `services::HudService` is the only HUD client in the applet (a field of `AppModel`).
  It owns auto-spawn, style updates (`style_message(&Config)`) and the detached `Quit`
  on drop. Delivery failures are logged at debug level instead of being discarded.
- `services::selector::select_region(RegionKind, snapshot)` replaces the two copied
  selector blocks. Four messages (`RoiSelectedWithSurface`, `RoiSelectionCancelled`,
  `IgnoreRoiSelectedWithSurface`, `IgnoreRoiSelectionCancelled`) became one
  `RegionSelectorFinished(RegionKind, Option<SelectorOutput>)`.
- `translator_core::ipc::SelectorOutput` defines the `ROI_SELECTED:` line; the overlay
  writes it and the applet parses it. The line format is unchanged.
- `services::process` holds executable lookup and the settings-window launch.
- `app.rs`: 1880 -> 1514 lines.

**A3. Applet modules.** Done.
- `settings/` has one module per tab (`capture`, `engine`, `display`, `glossary`,
  `history`), each with its own `Message`, `update` and `view`. Pages change `Config`
  directly and return `Update { task, request }`; `Request` covers the two actions
  that need app state (`SelectRegion`, `Close`). `SettingsMessage` has 7 variants
  instead of 38.
- Side effects of config changes are derived from the old and new config in
  `app::changes::ConfigDelta` (region, OCR, translation, HUD style). The same path
  handles edits in this process and changes picked up from `cosmic-config`, so a
  write echoed back by the config watcher no longer restarts the orchestrator, and a
  config change no longer starts the HUD.
- `app/` modules: `mod.rs` (model, messages, routing; 405 lines), `capture.rs`,
  `ocr.rs`, `translation.rs` (backend selection shared by the orchestrator and
  one-shot translation), `changes.rs`, `dashboard.rs`, `popup.rs`, `shortcuts.rs`.
  `update()` is one line per message.
- Removed messages without a sender or with a single caller: `CloseSettingsWindow`,
  `HudCleared`, `RoiSelected`, `IgnoreRoiSelected`, `SelectIgnoreRoiRequested`,
  `ClearIgnoreRoi`, `ClearHistory`, `CopyHistoryEntry`.

**A4. Extract crates.** In progress.
- `translator-protocol` done. It holds `hud.rs` (messages, socket path, config ID),
  `selector.rs`, `region.rs` (`Roi`, `RoiPreset`, `CaptureRegion`) and `logging.rs`.
  The socket client/server is behind the `ipc` feature, log setup behind `logging`.
  `HudIpcClient::send` returns a typed `SendError` instead of `anyhow::Error`.
- The pixel methods on `Roi` (`crop_to_rgba`, `mask_region`) became the
  `translator_core::capture::RoiPixels` trait, since `translator-core` can no longer add
  inherent methods to a type from another crate.
- The overlay depends only on `translator-protocol`. Measured with a clean
  `cargo build --release --locked -p cosmic-translator-overlay` in a separate target
  directory on an idle machine:

  | | before | after |
  |---|---|---|
  | build time | 143 s | 112 s |
  | binary size | 39.3 MB | 37.6 MB |
  | crates in the normal dependency tree | 541 | 489 |

  ocrs, rten, pipewire, ashpd and reqwest are no longer linked into the overlay. Most of
  the remaining time and size is libcosmic.
- `translator-translate` done. It holds the former `translation/` module and its three
  integration tests. Dependencies: anyhow, regex, reqwest, serde, serde_json, strsim;
  tokio only for tests. `translator-core` re-exports it as `translation`, so callers did
  not change, and dropped its own `regex` and `serde_json` dependencies.
- Then `translator-capture`, `translator-ocr` (with features).
- `translator-core` keeps the orchestrator and pipeline.

**A5. Library crates.**
- Give `cjk-ocr-layout` its own rectangle type with `From` conversions to and from `Roi`.
- Extract `screen-text-settle` and `cjk-ocr-layout`, write docs and examples, decide the
  license, add publish dry-run and semver checks to CI, publish 0.1.0.

**A6. Distribution.**
- Demo media, AUR package, Flatpak manifest.

A1 to A3 can be done in any order and give most of the day-to-day benefit. A4 and A5
should wait until A1 is done, because extraction before fixing the dependency direction
moves the coupling into `Cargo.toml` instead of removing it.

## 8. Not Planned

- One crate per module. Crates with a single small file add manifest and compile
  overhead without removing any dependency.
- Generic plugin systems for OCR or translation engines. The enum-plus-trait approach in
  `OcrEngineChoice` and `TranslationBackend` is sufficient for the known engines.
- Publishing the application crates on crates.io. They depend on `libcosmic` from git,
  which crates.io does not accept.
