//! Post-processing of OCR boxes for Japanese, Chinese and Korean text.
//!
//! Text detectors return one box per word or line, in no useful order. For CJK text,
//! turning those boxes into a readable string needs more than sorting by `y`, then `x`:
//!
//! - Vertical text (tategaki) is read top to bottom, columns right to left.
//! - Furigana (ruby) next to the main text is recognised as separate boxes and has to
//!   be removed, or the reading of every annotated word is inserted into the sentence.
//! - Tokens are joined without spaces, except between adjacent Latin words or numbers.
//! - Recognisers emit spaces between CJK characters, half-width punctuation and
//!   inconsistent widths, which have to be normalised before comparing or translating.
//!
//! The crate has no OCR engine dependency. Convert the boxes of any engine to
//! [`TextBox`] and run [`detect_dominant_orientation`], [`filter_furigana`],
//! [`sort_cjk_reading_order`], [`join_cjk_or_latin_tokens`] and [`normalize_cjk_text`]
//! in that order.
//!
//! # Example
//!
//! ```
//! use cjk_ocr_layout::{
//!     detect_dominant_orientation, filter_furigana, is_cjk_text, join_cjk_or_latin_tokens,
//!     normalize_cjk_text, sort_cjk_reading_order, Rect, TextBox, TextOrientation,
//! };
//!
//! // Two vertical columns and the furigana of the right one, in detector order.
//! let boxes = vec![
//!     TextBox::new(Rect::new(40, 50, 40, 80), "学習", 0.95),
//!     TextBox::new(Rect::new(142, 55, 12, 90), "にほんご", 0.85),
//!     TextBox::new(Rect::new(100, 50, 40, 120), "日本語", 0.95),
//! ];
//!
//! let orientation = detect_dominant_orientation(&boxes);
//! assert_eq!(orientation, TextOrientation::Vertical);
//!
//! let boxes = filter_furigana(boxes, orientation);
//! let boxes = sort_cjk_reading_order(boxes, orientation);
//! let tokens: Vec<&str> = boxes.iter().map(|b| b.text.as_str()).collect();
//! let cjk = boxes.iter().any(|b| is_cjk_text(&b.text));
//! let text = normalize_cjk_text(&join_cjk_or_latin_tokens(&tokens, cjk));
//! assert_eq!(text, "日本語学習");
//! ```
//!
//! # Features
//!
//! - `serde`: `Serialize` and `Deserialize` for [`Rect`], [`TextBox`] and
//!   [`TextOrientation`].

#![warn(missing_docs)]

pub mod furigana;
pub mod layout;
pub mod normalize;
pub mod rect;
pub mod script;

pub use furigana::{filter_furigana, is_punctuation_char};
pub use layout::{detect_dominant_orientation, sort_cjk_reading_order, TextBox, TextOrientation};
pub use normalize::normalize_cjk_text;
pub use rect::Rect;
pub use script::{is_cjk_char, is_cjk_text, join_cjk_or_latin_tokens};
