//! Japanese recognition through the PaddleOCR backend.
//!
//! Ignored by default: requires the PP-OCRv5 model set in
//! `$XDG_DATA_HOME/cosmic-ext-translator/models/ppocrv5/` (or network access to download
//! it from the project's `models-v1` release).
//! Run with `cargo test --release -p translator-core --test paddle_ja -- --ignored`.

use std::path::PathBuf;

use translator_core::ocr::{create_ocr_backend, OcrEngineChoice, OrientationMode, TextOrientation};

fn fixture(name: &str) -> image::DynamicImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/ja")
        .join(name);
    image::open(&path).unwrap_or_else(|e| panic!("cannot open {path:?}: {e}"))
}

const BOTH_LINES: &str = "明日は学校に行きます。勇者は古の剣を抜いた。";

#[tokio::test]
#[ignore = "requires the PP-OCRv5 model set"]
async fn reads_horizontal_vertical_dark_and_furigana_fixtures() {
    let backend = create_ocr_backend("ja", OrientationMode::Auto, OcrEngineChoice::Paddle)
        .await
        .expect("PP-OCRv5 models must be installed or downloadable");

    let cases = [
        ("horiz.png", BOTH_LINES, TextOrientation::Horizontal),
        ("horiz_dark.png", BOTH_LINES, TextOrientation::Horizontal),
        ("vert.png", BOTH_LINES, TextOrientation::Vertical),
        (
            "furigana.png",
            "勇者は剣を抜いた。",
            TextOrientation::Horizontal,
        ),
    ];
    for (file, expected, orientation) in cases {
        let result = backend
            .recognize_detailed(&fixture(file), None)
            .await
            .unwrap_or_else(|e| panic!("{file}: {e}"));
        assert_eq!(result.text, expected, "{file}");
        assert_eq!(result.orientation, orientation, "{file}");
    }
}
