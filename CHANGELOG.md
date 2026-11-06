# Changelog

All notable changes to this project are documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/). Release notes on GitHub are generated from
the section of the released version.

## [Unreleased]

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

- Interrupted or corrupt model downloads no longer leave OCR permanently broken.
- Desktop entry and AppStream metadata now pass `desktop-file-validate` and
  `appstreamcli validate`.

## [0.1.0] - 2026-11-03

Initial release.

[Unreleased]: https://github.com/Hasmolam/cosmic-ext-translator/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/Hasmolam/cosmic-ext-translator/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Hasmolam/cosmic-ext-translator/releases/tag/v0.1.0
