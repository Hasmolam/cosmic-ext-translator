# Japanese Text Support: Audit and Development Plan

Status: draft, 2026-11-06. Scope: reading Japanese (hiragana, katakana, kanji) from game
screens and passing it to the existing translation engines. Supersedes the OCR items of
Phase 2 in [COMPARATIVE_ANALYSIS_AND_ROADMAP.md](COMPARATIVE_ANALYSIS_AND_ROADMAP.md).

## 1. Current State

| Component | Japanese behaviour today |
|---|---|
| `OcrsBackend` (default) | Latin-only recognition model. Kana/kanji are not in its alphabet; output is empty or garbage. |
| `TesseractBackend` | Selected only when `source_lang` is exactly `ja`/`jpn` **and** the `tesseract` binary exists. Not installed by any packaging recipe. |
| `layout.rs` (vertical ordering), `furigana.rs` (ruby pruning) | Implemented and tested, but only called from `OcrsBackend::recognize_detailed`, which cannot read Japanese. Dead code for the Japanese path. |
| `join_cjk_or_latin_tokens` | Same: only reached through `OcrsBackend`. |
| Applet source language | Free-text input, default `auto`. `auto` always selects `OcrsBackend`. |
| Translation backends | Google/DeepL/OpenAI/Ollama accept `ja` already. No work needed here. |

Conclusion: Japanese reading is not functional in the shipped configuration. The README
already states this under Limitations.

## 2. Defects Found During the Audit

Ordered by impact. File references are at the time of writing.

### 2.1 Japanese-blocking

1. **OCR backend is not rebuilt when the source language changes.**
   `app.rs` `SettingsMessage::SourceLangChanged` only calls `restart_orchestrator_if_active()`,
   which reuses `self.ocr_backend`. The backend is chosen once in `ToggleCapture(true)`.
   Switching `auto` -> `ja` during a session keeps the Latin model until capture is restarted.

2. **Tesseract availability check ignores the language pack.**
   `TesseractBackend::is_available()` runs `tesseract --version` only. With the binary present
   but `tesseract-ocr-jpn` missing, every settled frame fails with
   `"Tesseract exited with non-zero status"`; stderr is sent to `/dev/null`, so the cause
   is never logged. Should check `tesseract --list-langs` and surface the missing pack.

3. **Tesseract output is not post-processed.**
   `TesseractBackend` implements only `recognize()`. The default `recognize_detailed()` returns
   no boxes, so furigana pruning, vertical ordering and CJK joining never run. Tesseract `jpn`
   output contains inter-character spaces and line breaks. Consequences:
   - `TextGate` prefix-expansion check (`starts_with`) fails when spacing differs between frames,
     so typewriter text is emitted as several partial translations.
   - Translation engines receive `私 は 学 生 で す` style input.

4. **No vertical mode for Tesseract.** Always uses `jpn` with default PSM 3. Vertical
   (tategaki) text needs `jpn_vert` and PSM 5; a dialogue box needs PSM 6.

5. **`source_lang` is unvalidated.** `jp`, `japanese`, `JA ` (trailing space) silently fall
   back to the Latin backend.

### 2.2 General defects

6. **Mock fallback is visible to the user as a subtitle.** If model load fails,
   `create_ocr_backend_for_lang` and `ToggleCapture(true)` substitute
   `MockOcrBackend::new("Capture Active")`. The pipeline then OCRs every frame as
   `"Capture Active"`, translates it once and shows that translation in the HUD. The user
   sees a plausible subtitle instead of an error.

7. **Model download is not atomic and not verified.** `load_default_or_download` writes the
   response directly to the final path and only checks `len >= 100_000`. An interrupted
   download larger than 100 KB stays on disk, `Model::load_file` fails on every start, and
   finding 6 hides it.

7a. **Model download URLs are dead.** (Found during Phase 0.) The `.rten` URLs on
   `ocrs-models.s3-accelerate.amazonaws.com` return HTTP 403 `AccessDenied`. Upstream ocrs
   now downloads `text-detection.onnx` / `text-recognition.onnx` from the same bucket.
   Fresh installs could not obtain models at all.

8. **Hard-coded Turkish strings with emoji in `translator-core`.**
   `orchestrator/mod.rs` lines 227, 275, 287 send `"Hız Sınırı Aşıldı ..."` and
   `"Çeviri Hatası ..."` to the HUD regardless of UI language. The core crate has no i18n;
   it should send a typed status (`HudMessage::SetStatus` with a code) and let the overlay
   localise it. The DeepL key placeholder in `settings_dialog.rs` has the same problem.

9. **Per-keystroke restarts.** Every character typed in source/target language, model or
   endpoint fields calls `save_config()` and restarts the orchestrator (which, after fixing
   finding 1, would also rebuild the OCR backend). Replace the language text inputs with a
   dropdown and apply other text fields on submit or after a short delay.

10. **Blocking call in async context.** `TesseractBackend::is_available()` uses
    `std::process::Command` and is called from the async `create_ocr_backend_for_lang`.
    Use `tokio::process` or `spawn_blocking`.

11. **Cache and jitter thresholds assume Latin text density.** `FuzzyLruCache` (0.90) and
    `TextGate` jitter (0.92) use character-level normalised Levenshtein. A 10-character
    Japanese line that differs in one character (e.g. `明日は学校に行きます` vs
    `明日は学校に行けます`, "I will go to school tomorrow" vs "I can go ...") scores 0.90 and is served from cache. Thresholds should depend on
    script and length (for CJK: require exact match below ~20 characters).

12. **HUD socket takeover.** `HudIpcServer::start` deletes an existing socket without testing
    whether a live HUD owns it. A second HUD orphans the first. Low severity: probe with
    `UnixStream::connect` first. The `/tmp` fallback path is also shared between users.

### 2.3 Build environment (not a code defect)

`cargo clippy` / `cargo test` cannot run on the current workstation: `libxkbcommon-dev` and
`libpipewire-0.3-dev` (pkg-config `xkbcommon`, `libpipewire-0.3`) are not installed. Earlier
builds in `target/` predate their removal. Install them before working on the phases below:

```sh
sudo apt install libxkbcommon-dev libpipewire-0.3-dev libspa-0.2-dev
```

## 3. OCR Engine Options for Japanese

| Option | Accuracy on game text | Runtime dependency | Vertical text | Fit with current design |
|---|---|---|---|---|
| Tesseract `jpn`/`jpn_vert` (CLI) | Low-medium; poor on small or outlined fonts | System package | Yes (`jpn_vert`) | Already wired; quick to fix |
| PaddleOCR PP-OCRv4/v5 det + `japan` rec, run through `rten` | Medium-high | None (model files only) | Partial; needs rotated-line handling | Same engine as `ocrs`; keeps the pure-Rust build |
| PaddleOCR via `ort` (ONNX Runtime) | Same as above | `libonnxruntime.so` | Partial | Adds a C++ runtime; avoid unless `rten` lacks operators |
| manga-ocr (ViT encoder + autoregressive decoder) | High on manga/VN dialogue, handles vertical and furigana natively | ~400 MB model | Yes | Line/block recogniser only; needs a detector in front; slow on CPU |

Recommendation: fix the Tesseract path first so users get working Japanese quickly, make
PaddleOCR-on-`rten` the bundled default after a feasibility spike, and treat manga-ocr as an
optional high-accuracy mode.

## 4. Phases

Each phase ends with `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo test --workspace`, plus the manual check listed under "Done when".

### Phase 0: Pre-work (defects 1, 5, 6, 7, 7a, 8) -- implemented 2026-11-06

Status: implemented, not yet tested inside a live COSMIC session (needs the screencast
portal). Verified headlessly: backend selection for `ja`/`jp`/`auto`, fresh model download,
reuse of existing `.rten` files, quarantine and re-download of a truncated model, OCR output
identical between `.onnx` and `.rten` models on `docs/assets/applet_popup.png`.

Deviations from the original bullet list:

- No pinned SHA-256. The upstream model URLs are unversioned; a pinned hash would break
  every install the next time upstream replaces a model. Validation is done by parsing the
  downloaded file with `rten` before it is renamed into place, plus an ONNX header check
  (`0x08`, `ir_version`) because `rten-onnx` 0.26 spins indefinitely on some malformed input
  and accepts an all-zero file. Parsing is also bounded by a 30 s timeout.
- Temporary files are named `<stem>.part.<ext>`: `rten` picks the parser from the file
  extension, so `model.onnx.part` would always fail to load.
- `create_ocr_backend_for_lang` already checks `tesseract --list-langs` (defect 2) and runs
  it through `tokio::process` (defect 10), because a correct error message for Japanese
  depends on it. TSV parsing and post-processing remain in Phase 1.
- The rate-limit badge was never cleared after a 429 (no `SetStatus { active: false }` was
  ever sent). Fixed together with the typed status messages.

Original scope:

- Introduce `SourceLanguage` enum in `translator-core` (`Auto`, `English`, `Japanese`,
  `ChineseSimplified`, `Korean`, ...) with `from_code()` accepting `ja`, `jp`, `jpn`,
  `japanese` case-insensitively. Keep the ISO code in config for compatibility.
- Replace the source-language `text_input` with a `dropdown`.
- On `SourceLangChanged`, rebuild the OCR backend (async task), then restart the orchestrator.
- Replace the `MockOcrBackend` fallback with an `OcrUnavailable` state: no pipeline,
  HUD status message, applet popup shows the reason.
- Atomic, checksummed model downloads into a shared helper (Phase 2 reuses it).
- Move HUD status strings out of `translator-core`: add `HudStatus` enum
  (`RateLimited { secs }`, `TranslationFailed`, `OcrUnavailable { reason }`), localise in the
  overlay's Fluent files (en, tr).

Done when: switching source language during an active capture changes the backend (log line),
and deleting the model directory produces a visible error instead of a subtitle.

### Phase 1: Working Japanese through Tesseract (defects 2, 3, 4, 10)

- `TesseractBackend`:
  - `probe()` -> `Result<TesseractCaps, OcrError>` using `tesseract --list-langs`; report a
    missing `jpn` / `jpn_vert` pack by name.
  - Capture stderr; include it in the error.
  - Use TSV output (`tesseract stdin stdout -l jpn --psm 6 tsv`) and implement
    `recognize_detailed()`: build `DetectedTextBox` per word/line from TSV rows, then run the
    existing `filter_furigana` -> `sort_cjk_reading_order` -> `join_cjk_or_latin_tokens`
    chain. Extract that chain into one function shared with `OcrsBackend`.
  - Orientation setting: `Auto | Horizontal | Vertical`. Vertical uses `jpn_vert --psm 5`.
    Auto uses `detect_dominant_orientation` on a first horizontal pass.
- Add `normalize_cjk_text()`: remove whitespace between CJK characters, convert half-width
  katakana and full-width ASCII to canonical forms (NFKC), keep `、。！？「」` intact. Apply
  before `TextGate` and the cache.
- Switch the pipeline from `recognize()` to `recognize_detailed()` so orientation and boxes are
  available downstream (debug overlay, future in-place rendering).
- Packaging: add `tesseract-ocr` and `tesseract-ocr-jpn`, `tesseract-ocr-jpn-vert` as
  *Recommends* in the `.deb`, `optdepends` (`tesseract`, `tesseract-data-jpn`,
  `tesseract-data-jpn_vert`) in the PKGBUILD; `install.sh` prints the command when missing.
- Tests (no Tesseract binary required in CI):
  - TSV parser fixtures for horizontal, vertical and furigana-bearing samples.
  - `normalize_cjk_text` table tests.
  - `TextGate` typewriter sequence with spaced Tesseract output (`私 は` -> `私は学生`).
  - Optional `#[ignore]` integration test that runs real Tesseract on PNG fixtures in
    `crates/translator-core/tests/fixtures/ja/`.

Done when: with `tesseract-ocr-jpn` installed, a Ren'Py or RPG Maker test scene in Japanese
produces one translation per dialogue line, with furigana removed, and the applet reports a
missing language pack by name when it is not installed.

### Phase 2: Bundled Japanese model on `rten` (spike first)

Spike (time-boxed, 2-3 days), result recorded in this file:

1. Export PaddleOCR PP-OCRv4 (or v5) detection and `japan` recognition models to ONNX
   (`paddle2onnx`), convert with `rten-convert`, load with `rten::Model::load_file`.
2. Confirm all operators run on `rten` 0.26 and measure latency on a 1280x200 dialogue ROI
   (target: < 150 ms on a 4-core laptop CPU with dev profile overrides).
3. Compare character error rate against Tesseract on 30 captured frames (VN, JRPG, pixel font).

If the spike passes:

- New module `ocr/paddle.rs`: `PaddleOcrBackend` with DB post-processing (threshold,
  unclip, min-area box), per-line crop and resize to 48 px height, CTC greedy decoding with
  the `japan_dict.txt` character table.
- Vertical lines: detect `h / w > 1.5` boxes, rotate 90 degrees before recognition.
- Model manager: model sets keyed by script (`latin` -> ocrs, `japanese` -> paddle),
  downloaded on first use to `$XDG_DATA_HOME/cosmic-ext-translator/models/<set>/` with the
  Phase 0 download helper (`ocr::models`); system path `/usr/share/cosmic-ext-translator/models/<set>/` first.
- Licence check: PaddleOCR models are Apache-2.0; record attribution in `README.md` and
  packaging metadata.
- Backend selection order for Japanese: Paddle (if models present) -> Tesseract -> error state.
  User can force an engine in the OCR settings tab.
- Add `opt-level = 3` overrides for any new inference crates, as with `ocrs`/`rten`.

If the spike fails on operator coverage: evaluate `ort` behind a cargo feature
(`onnx-runtime`), off by default, and keep Tesseract as the default Japanese path.

Done when: a fresh install without Tesseract reads Japanese after the one-time model download.

### Phase 3: Japanese-specific quality

- Script-aware thresholds (defect 11): `TextGate` and `FuzzyLruCache` take a `ScriptProfile`;
  CJK lines under 20 characters require an exact match in the cache.
- Speaker tags: extend `speaker.rs` to `【名前】`, `「...」` after a name line, and
  `名前：` (full-width colon). Add fixtures.
- Glossary: match on normalised text (NFKC) so half-width/full-width variants hit the same entry.
- Context prompt for LLM backends: state that input is Japanese game dialogue, may omit
  subjects, and that honorifics should be kept or mapped per a user setting.
- HUD: optional "show original" line rendered with a CJK-capable font fallback
  (check that the COSMIC default font set covers kanji; otherwise document `fonts-noto-cjk`).

### Phase 4: Optional high-accuracy mode (manga-ocr)

Only after Phase 2. Run manga-ocr encoder/decoder through `rten` if operators allow, behind
an "accurate (slow)" toggle, using Phase 2 detection boxes as input crops. Measure CPU latency
before committing; skip if > 1 s per dialogue box.

## 5. Out of Scope

- Chinese and Korean recognition models (the enum and model-set structure from Phases 0 and 2
  make them a follow-up, not part of this request).
- Furigana-to-reading extraction, dictionary lookup, Anki export.
- Text hooking / memory reading.

## 6. Order of Work

1. Install missing build dependencies (section 2.3).
2. Phase 0 (one PR per bullet group: language enum + dropdown, backend rebuild, error state,
   downloads, HUD status i18n).
3. Phase 1, then release as `0.2.0` with "Japanese via Tesseract" in the release notes.
4. Phase 2 spike; decide on the Paddle path; implement.
5. Phase 3 items as separate small PRs.
