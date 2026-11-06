//! Japanese recognition through the real Tesseract binary.
//!
//! Ignored by default: requires `tesseract` with `jpn` and `jpn_vert` language data.
//! Run with `cargo test -p translator-core --test tesseract_ja -- --ignored`.

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
#[ignore = "requires tesseract with jpn and jpn_vert language data"]
async fn auto_mode_reads_horizontal_vertical_and_dark_text() {
    let backend = create_ocr_backend("ja", OrientationMode::Auto, OcrEngineChoice::Tesseract)
        .await
        .expect("tesseract with jpn and jpn_vert must be installed");

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

#[tokio::test]
#[ignore = "requires tesseract with jpn and jpn_vert language data"]
async fn forced_vertical_mode_reads_vertical_text() {
    let backend = create_ocr_backend("ja", OrientationMode::Vertical, OcrEngineChoice::Tesseract)
        .await
        .expect("tesseract with jpn and jpn_vert must be installed");
    let result = backend
        .recognize_detailed(&fixture("vert.png"), None)
        .await
        .expect("vertical recognition");
    assert_eq!(result.text, BOTH_LINES);
}
