//! `TextGate` on Tesseract output after CJK normalisation.

use translator_core::ocr::normalize_cjk_text;
use translator_core::pipeline::debouncer::{TextDiffAction, TextGate};

#[test]
fn test_text_gate_typewriter_with_spaced_tesseract_output() {
    let mut gate = TextGate::new(0.92);

    // Tesseract spacing differs between frames; normalisation makes them prefixes.
    let frames = ["私 は", "私は 学生", "私 は 学生 です 。"];
    let first = normalize_cjk_text(frames[0]);
    assert_eq!(gate.evaluate(&first), TextDiffAction::EmitTranslation);
    for (i, raw) in frames.iter().enumerate().skip(1) {
        let text = normalize_cjk_text(raw);
        assert_eq!(
            gate.evaluate(&text),
            TextDiffAction::TypewriterWait,
            "frame {i} ({text}) should extend the previous line"
        );
    }
    assert_eq!(gate.flush_pending().as_deref(), Some("私は学生です。"));
}
