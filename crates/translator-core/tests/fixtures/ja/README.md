# Japanese OCR fixtures

Rendered with `text2image` (Tesseract 5.3.4) using Noto Sans CJK JP Medium.

| File | Content |
| :--- | :--- |
| `horiz.png` | Two horizontal lines: `明日は学校に行きます。` / `勇者は古の剣を抜いた。` |
| `horiz_dark.png` | `horiz.png` with light text on a dark background |
| `vert.png` | The same two sentences written vertically (`--writing_mode=vertical`) |
| `furigana.png` | `勇者は剣を抜いた。` with the ruby `ゆうしゃ` composited above `勇者` |

Used by `tests/tesseract_ja.rs`, which is ignored by default because it needs the
`tesseract` binary with `jpn` and `jpn_vert` language data:

```sh
cargo test -p translator-core --test tesseract_ja -- --ignored
```
