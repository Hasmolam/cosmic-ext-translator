//! Runs the full post-processing chain on boxes a detector could return for a vertical
//! Japanese text block with furigana, and prints each step.
//!
//! `cargo run -p cjk-ocr-layout --example vertical_page`

use cjk_ocr_layout::{
    detect_dominant_orientation, filter_furigana, is_cjk_text, join_cjk_or_latin_tokens,
    normalize_cjk_text, sort_cjk_reading_order, Rect, TextBox,
};

fn main() {
    // Three columns, read right to left: 日本語を / 勉強して / います。
    // 日本語 and 勉強 carry furigana in thin columns to their right. The detector
    // returns the boxes in arbitrary order and puts a space inside one column.
    let boxes = vec![
        TextBox::new(Rect::new(40, 50, 40, 150), "います 。", 0.93),
        TextBox::new(Rect::new(182, 55, 12, 90), "にほんご", 0.81),
        TextBox::new(Rect::new(140, 50, 40, 160), "日本語を", 0.95),
        TextBox::new(Rect::new(90, 50, 40, 150), "勉強して", 0.94),
        TextBox::new(Rect::new(132, 52, 11, 60), "べんきょう", 0.80),
    ];
    print_step("detector output", &boxes);

    let orientation = detect_dominant_orientation(&boxes);
    println!("orientation: {orientation:?}\n");

    let boxes = filter_furigana(boxes, orientation);
    print_step("without furigana", &boxes);

    let boxes = sort_cjk_reading_order(boxes, orientation);
    print_step("reading order", &boxes);

    let tokens: Vec<&str> = boxes.iter().map(|b| b.text.as_str()).collect();
    let cjk = boxes.iter().any(|b| is_cjk_text(&b.text));
    let joined = join_cjk_or_latin_tokens(&tokens, cjk);
    println!("joined:     {joined}");
    println!("normalised: {}", normalize_cjk_text(&joined));
}

fn print_step(title: &str, boxes: &[TextBox]) {
    println!("{title}:");
    for b in boxes {
        let r = b.rect;
        println!(
            "  x={:>3} y={:>3} {:>3}x{:<3} {}",
            r.x, r.y, r.width, r.height, b.text
        );
    }
    println!();
}
