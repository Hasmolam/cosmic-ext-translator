//! Canonicalisation of OCR text before it reaches the text gate, cache and translators.
//!
//! OCR engines emit the same on-screen text in slightly different forms from frame to
//! frame: Tesseract inserts spaces between Japanese tokens, games mix half-width and
//! full-width characters. Without normalisation the typewriter prefix check and the fuzzy
//! cache treat these variants as different strings.

use super::is_cjk_char;

/// Normalises OCR output:
///
/// 1. Full-width ASCII letters and digits (`Ａ`, `１`) become ASCII.
/// 2. Half-width katakana and punctuation (`ｶﾞ`, `｡`) become full-width (`ガ`, `。`).
/// 3. Whitespace adjacent to a CJK character is removed; any other whitespace run,
///    including line breaks, collapses to one ASCII space.
///
/// Full-width punctuation such as `！？「」` is preserved.
#[must_use]
pub fn normalize_cjk_text(input: &str) -> String {
    let folded = fold_width(input);

    let mut out = String::with_capacity(folded.len());
    let mut pending_space = false;
    for c in folded.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            let prev_cjk = out.chars().last().is_some_and(is_cjk_char);
            if !prev_cjk && !is_cjk_char(c) {
                out.push(' ');
            }
            pending_space = false;
        }
        out.push(c);
    }
    out
}

/// Applies the width folding steps (1) and (2) of [`normalize_cjk_text`].
fn fold_width(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            // Full-width digits and Latin letters.
            '\u{FF10}'..='\u{FF19}' | '\u{FF21}'..='\u{FF3A}' | '\u{FF41}'..='\u{FF5A}' => {
                out.push(char::from_u32(c as u32 - 0xFEE0).unwrap_or(c));
            }
            // Half-width voiced / semi-voiced sound marks combine with the previous kana.
            '\u{FF9E}' | '\u{FF9F}' => {
                let semi = c == '\u{FF9F}';
                match out.pop() {
                    Some(prev) => match apply_sound_mark(prev, semi) {
                        Some(combined) => out.push(combined),
                        None => {
                            out.push(prev);
                            out.push(if semi { '\u{309C}' } else { '\u{309B}' });
                        }
                    },
                    None => out.push(if semi { '\u{309C}' } else { '\u{309B}' }),
                }
            }
            '\u{FF61}'..='\u{FF9D}' => out.push(halfwidth_to_fullwidth(c)),
            _ => out.push(c),
        }
    }
    out
}

/// Maps U+FF61..=U+FF9D to the corresponding full-width character.
fn halfwidth_to_fullwidth(c: char) -> char {
    const TABLE: [char; 61] = [
        '。', '「', '」', '、', '・', 'ヲ', 'ァ', 'ィ', 'ゥ', 'ェ', 'ォ', 'ャ', 'ュ', 'ョ', 'ッ',
        'ー', 'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ',
        'ソ', 'タ', 'チ', 'ツ', 'テ', 'ト', 'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ',
        'ホ', 'マ', 'ミ', 'ム', 'メ', 'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ', 'ル', 'レ', 'ロ', 'ワ',
        'ン',
    ];
    let idx = (c as u32).wrapping_sub(0xFF61) as usize;
    TABLE.get(idx).copied().unwrap_or(c)
}

/// Combines a full-width katakana with a (semi-)voiced sound mark, e.g. `カ` + `ﾞ` = `ガ`.
fn apply_sound_mark(base: char, semi_voiced: bool) -> Option<char> {
    let code = base as u32;
    let is_ha_row = (0x30CF..=0x30DD).contains(&code) && (code - 0x30CF).is_multiple_of(3);
    if semi_voiced {
        return is_ha_row.then(|| char::from_u32(code + 2)).flatten();
    }
    let voiceable = matches!(
        base,
        'カ' | 'キ'
            | 'ク'
            | 'ケ'
            | 'コ'
            | 'サ'
            | 'シ'
            | 'ス'
            | 'セ'
            | 'ソ'
            | 'タ'
            | 'チ'
            | 'ツ'
            | 'テ'
            | 'ト'
    ) || is_ha_row;
    if voiceable {
        char::from_u32(code + 1)
    } else if base == 'ウ' {
        Some('ヴ')
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_spaces_between_japanese_tokens() {
        assert_eq!(normalize_cjk_text("明日 は 学校 に"), "明日は学校に");
        assert_eq!(normalize_cjk_text("行き ます 。"), "行きます。");
    }

    #[test]
    fn joins_lines_without_breaking_sentences() {
        assert_eq!(
            normalize_cjk_text("明日は学校に行きま\nす。\n勇者は古の剣を抜い\nた。"),
            "明日は学校に行きます。勇者は古の剣を抜いた。"
        );
    }

    #[test]
    fn keeps_spaces_between_latin_words() {
        assert_eq!(normalize_cjk_text("Level  10\nup"), "Level 10 up");
        assert_eq!(normalize_cjk_text("  The hero  "), "The hero");
    }

    #[test]
    fn drops_space_at_latin_cjk_boundary() {
        assert_eq!(
            normalize_cjk_text("Level 10 へ ようこそ"),
            "Level 10へようこそ"
        );
        assert_eq!(normalize_cjk_text("HP が 0"), "HPが0");
    }

    #[test]
    fn folds_fullwidth_alphanumerics() {
        assert_eq!(normalize_cjk_text("ＨＰ１００"), "HP100");
        assert_eq!(normalize_cjk_text("ｌｖ．５"), "lv．5");
    }

    #[test]
    fn folds_halfwidth_katakana_with_sound_marks() {
        assert_eq!(normalize_cjk_text("ｶﾞﾝﾀﾞﾑ"), "ガンダム");
        assert_eq!(normalize_cjk_text("ﾊﾟｰﾃｨｰ"), "パーティー");
        assert_eq!(normalize_cjk_text("ｳﾞｧｲｵﾘﾝ"), "ヴァイオリン");
        assert_eq!(normalize_cjk_text("ｺﾝﾆﾁﾊ｡"), "コンニチハ。");
        assert_eq!(normalize_cjk_text("｢ﾎﾟｰｼｮﾝ｣"), "「ポーション」");
    }

    #[test]
    fn stray_sound_marks_are_preserved() {
        assert_eq!(normalize_cjk_text("ﾞ"), "\u{309B}");
        assert_eq!(normalize_cjk_text("ｱﾟ"), "ア\u{309C}");
    }

    #[test]
    fn preserves_fullwidth_punctuation() {
        assert_eq!(normalize_cjk_text("本当？！「はい」"), "本当？！「はい」");
    }

    #[test]
    fn ideographic_space_is_whitespace() {
        assert_eq!(normalize_cjk_text("勇者\u{3000}アレン"), "勇者アレン");
    }

    #[test]
    fn empty_and_whitespace_only() {
        assert_eq!(normalize_cjk_text(""), "");
        assert_eq!(normalize_cjk_text(" \n\t"), "");
    }
}
