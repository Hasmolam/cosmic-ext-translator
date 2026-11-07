# cjk-ocr-layout

Post-processing of OCR boxes for Japanese, Chinese and Korean text: vertical reading
order, furigana removal, token joining and normalisation. No OCR engine dependency.

Text detectors return one box per word or line, in no useful order. For CJK text,
sorting by `y`, then `x` is not enough:

- Vertical text (tategaki) is read top to bottom, columns right to left.
- Furigana (ruby) next to the main text is recognised as separate boxes. Left in, the
  reading of every annotated word ends up inside the sentence.
- Tokens are joined without spaces, except between adjacent Latin words or numbers.
- Recognisers put spaces between CJK characters and mix half-width and full-width
  forms.

| Function | Step |
|---|---|
| `detect_dominant_orientation` | Horizontal or vertical, by aspect ratio of the boxes. |
| `filter_furigana` | Drops small boxes right of a column (vertical) or above a line (horizontal). |
| `sort_cjk_reading_order` | Columns right to left, or lines top to bottom, then within each. |
| `join_cjk_or_latin_tokens` | Joins tokens without spaces for CJK, with spaces for Latin. |
| `normalize_cjk_text` | Removes whitespace next to CJK characters, folds full-width ASCII to ASCII and half-width katakana and punctuation to full-width. |

## Usage

```toml
[dependencies]
cjk-ocr-layout = "0.1"
```

Convert the boxes of your engine (`ocrs`, `oar-ocr`, `paddle-ocr-rs`, Tesseract TSV
output, ...) to `TextBox`:

```rust
use cjk_ocr_layout::{
    detect_dominant_orientation, filter_furigana, is_cjk_text, join_cjk_or_latin_tokens,
    normalize_cjk_text, sort_cjk_reading_order, Rect, TextBox,
};

let boxes: Vec<TextBox> = engine_output
    .iter()
    .map(|w| TextBox::new(Rect::new(w.x, w.y, w.width, w.height), &w.text, w.score))
    .collect();

let orientation = detect_dominant_orientation(&boxes);
let boxes = sort_cjk_reading_order(filter_furigana(boxes, orientation), orientation);
let tokens: Vec<&str> = boxes.iter().map(|b| b.text.as_str()).collect();
let cjk = boxes.iter().any(|b| is_cjk_text(&b.text));
let text = normalize_cjk_text(&join_cjk_or_latin_tokens(&tokens, cjk));
```

`examples/vertical_page.rs` prints each step for a vertical block with furigana:

```sh
cargo run -p cjk-ocr-layout --example vertical_page
```

```text
reading order:
  x=140 y= 50  40x160 日本語を
  x= 90 y= 50  40x150 勉強して
  x= 40 y= 50  40x150 います 。

joined:     日本語を勉強しています 。
normalised: 日本語を勉強しています。
```

## Features

- `serde`: `Serialize` and `Deserialize` for `Rect`, `TextBox` and `TextOrientation`.

## Limitations

- Furigana detection is geometric: a box under 35 % of the median box area that sits
  above a line or to the right of a column is removed, unless it is punctuation. A
  main-text box that small in that position is removed as well.
- Orientation is decided once for the whole set of boxes. Pages that mix vertical and
  horizontal blocks should be split by block first.
- Hangul counts as CJK. `join_cjk_or_latin_tokens(.., true)` and `normalize_cjk_text`
  remove the spaces between Korean words; for Korean, join with `is_cjk = false` and
  skip `normalize_cjk_text`.

## Origin

Extracted from [cosmic-ext-translator](https://github.com/Hasmolam/cosmic-ext-translator),
a screen translator for the COSMIC desktop.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual
licensed as above, without any additional terms or conditions.
