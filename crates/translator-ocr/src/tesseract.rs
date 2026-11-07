//! Tesseract CLI backend for scripts the bundled `ocrs` model cannot read (Japanese,
//! Chinese, Korean).
//!
//! Tesseract runs in TSV mode so each recognised word comes with a bounding box and a
//! confidence. Words are grouped into line (or column) boxes and passed through the same
//! furigana pruning, reading-order sorting and normalisation as the `ocrs` backend.

use std::process::Stdio;

use anyhow::{anyhow, bail, Result};
use image::DynamicImage;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use super::layout::{DetectedTextBox, OrientationMode, TextOrientation};
use super::{
    assemble_result, is_cjk_text, join_cjk_or_latin_tokens, BoxFuture, OcrBackend, OcrResult,
};
use translator_protocol::Roi;

/// Page segmentation mode: a single uniform block of horizontal text (dialogue boxes).
const PSM_HORIZONTAL: &str = "6";
/// Page segmentation mode: a single uniform block of vertically aligned text.
const PSM_VERTICAL: &str = "5";
/// In `Auto` mode, a horizontal pass at or above this mean confidence is accepted
/// without trying the vertical model.
const AUTO_ACCEPT_CONFIDENCE: f32 = 85.0;
/// Maximum length of Tesseract stderr included in an error message.
const STDERR_EXCERPT: usize = 300;

/// Tesseract OCR backend.
#[derive(Debug, Clone)]
pub struct TesseractBackend {
    /// Language pack for horizontal text, e.g. `jpn`.
    pub lang: String,
    /// Language pack for vertical text, e.g. `jpn_vert`, if installed.
    pub vertical_lang: Option<String>,
    pub mode: OrientationMode,
}

impl TesseractBackend {
    pub fn new(lang: impl Into<String>) -> Self {
        Self {
            lang: lang.into(),
            vertical_lang: None,
            mode: OrientationMode::Auto,
        }
    }

    #[must_use]
    pub fn with_vertical_lang(mut self, lang: impl Into<String>) -> Self {
        self.vertical_lang = Some(lang.into());
        self
    }

    #[must_use]
    pub fn with_mode(mut self, mode: OrientationMode) -> Self {
        self.mode = mode;
        self
    }

    /// Returns true if a `tesseract` binary can be executed.
    pub fn is_available() -> bool {
        std::process::Command::new("tesseract")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Lists installed Tesseract language packs, or `None` if Tesseract cannot be run.
    pub async fn installed_languages() -> Option<Vec<String>> {
        let output = tokio::process::Command::new("tesseract")
            .arg("--list-langs")
            .stdin(Stdio::null())
            .output()
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        // Older releases print the list to stderr, newer ones to stdout.
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push('\n');
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Some(parse_tesseract_lang_list(&text))
    }

    async fn run_pass(&self, png: &[u8], orientation: TextOrientation) -> Result<Pass> {
        let (lang, psm) = match orientation {
            TextOrientation::Horizontal => (self.lang.as_str(), PSM_HORIZONTAL),
            TextOrientation::Vertical => (
                self.vertical_lang.as_deref().unwrap_or(self.lang.as_str()),
                PSM_VERTICAL,
            ),
        };
        let tsv = run_tesseract_tsv(png, lang, psm).await?;
        let words = parse_tsv(&tsv);
        let pass = Pass::new(orientation, &words);
        debug!(
            "Tesseract {lang} psm {psm}: {} lines, mean confidence {:.1}",
            pass.lines.len(),
            pass.confidence
        );
        Ok(pass)
    }
}

/// Parses `tesseract --list-langs` output, skipping the header line.
#[must_use]
pub fn parse_tesseract_lang_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("List of available languages"))
        .map(str::to_string)
        .collect()
}

impl OcrBackend for TesseractBackend {
    fn recognize<'a>(&'a self, image: &'a DynamicImage) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { Ok(self.recognize_detailed(image, None).await?.text) })
    }

    fn recognize_detailed<'a>(
        &'a self,
        image: &'a DynamicImage,
        orientation: Option<TextOrientation>,
    ) -> BoxFuture<'a, Result<OcrResult>> {
        Box::pin(async move {
            let png = encode_png(image)?;
            let pass = match orientation.or(self.mode.forced()) {
                Some(fixed) => self.run_pass(&png, fixed).await?,
                None => {
                    let horizontal = self.run_pass(&png, TextOrientation::Horizontal).await?;
                    if self.vertical_lang.is_some() && horizontal.is_doubtful() {
                        let vertical = self.run_pass(&png, TextOrientation::Vertical).await?;
                        choose_pass(horizontal, vertical)
                    } else {
                        horizontal
                    }
                }
            };
            Ok(assemble_result(pass.lines, Some(pass.orientation), true))
        })
    }
}

/// Result of one Tesseract run.
#[derive(Debug)]
struct Pass {
    orientation: TextOrientation,
    lines: Vec<DetectedTextBox>,
    /// Mean word confidence (0-100), weighted by character count.
    confidence: f32,
}

impl Pass {
    fn new(orientation: TextOrientation, words: &[TsvWord]) -> Self {
        Self {
            orientation,
            lines: group_lines(words),
            confidence: mean_confidence(words),
        }
    }

    /// True if a horizontal result should be compared against a vertical pass.
    ///
    /// Low confidence alone is not enough: a partially revealed horizontal line (typewriter
    /// effect, glyph cut at the edge) also scores low, and the vertical model can return a
    /// garbled string with a higher score. A retry needs geometric evidence of vertical
    /// text: stacked one-character lines, or a text area taller than it is wide.
    fn is_doubtful(&self) -> bool {
        if self.lines.is_empty() {
            return false;
        }
        looks_like_stacked_columns(&self.lines)
            || (self.confidence < AUTO_ACCEPT_CONFIDENCE && is_taller_than_wide(&self.lines))
    }
}

/// Picks the pass with higher confidence; prefers horizontal on ties.
fn choose_pass(horizontal: Pass, vertical: Pass) -> Pass {
    if vertical.confidence > horizontal.confidence {
        debug!(
            "Tesseract auto orientation: vertical ({:.1}) over horizontal ({:.1})",
            vertical.confidence, horizontal.confidence
        );
        vertical
    } else {
        horizontal
    }
}

/// A horizontal pass over vertical text yields many one-character lines stacked on top
/// of each other.
fn looks_like_stacked_columns(lines: &[DetectedTextBox]) -> bool {
    if lines.len() < 3 {
        return false;
    }
    let single = lines.iter().filter(|l| l.text.chars().count() == 1).count();
    single * 10 >= lines.len() * 6
}

/// True if the union of all line boxes is taller than it is wide.
fn is_taller_than_wide(lines: &[DetectedTextBox]) -> bool {
    let left = lines.iter().map(|l| l.rect.x).min().unwrap_or(0);
    let top = lines.iter().map(|l| l.rect.y).min().unwrap_or(0);
    let right = lines
        .iter()
        .map(|l| l.rect.x.saturating_add(l.rect.width))
        .max()
        .unwrap_or(0);
    let bottom = lines
        .iter()
        .map(|l| l.rect.y.saturating_add(l.rect.height))
        .max()
        .unwrap_or(0);
    bottom.saturating_sub(top) > right.saturating_sub(left)
}

/// One word row (TSV level 5) from Tesseract output.
#[derive(Debug, Clone, PartialEq)]
pub struct TsvWord {
    pub block: u32,
    pub paragraph: u32,
    pub line: u32,
    pub rect: Roi,
    /// Confidence in percent (0-100).
    pub confidence: f32,
    pub text: String,
}

/// Parses Tesseract TSV output into word rows. Header, non-word rows and malformed rows
/// are skipped.
#[must_use]
pub fn parse_tsv(tsv: &str) -> Vec<TsvWord> {
    let mut words = Vec::new();
    for (idx, row) in tsv.lines().enumerate() {
        let cols: Vec<&str> = row.split('\t').collect();
        if cols.len() < 12 || cols[0] != "5" {
            continue;
        }
        let text = cols[11..].join("\t").trim().to_string();
        if text.is_empty() {
            continue;
        }
        let num = |i: usize| cols[i].trim().parse::<u32>().ok();
        let (Some(block), Some(paragraph), Some(line)) = (num(2), num(3), num(4)) else {
            debug!("Skipping malformed Tesseract TSV row {idx}: {row}");
            continue;
        };
        let (Some(x), Some(y), Some(width), Some(height)) = (num(6), num(7), num(8), num(9)) else {
            debug!("Skipping malformed Tesseract TSV row {idx}: {row}");
            continue;
        };
        let confidence = cols[10]
            .trim()
            .parse::<f32>()
            .unwrap_or(0.0)
            .clamp(0.0, 100.0);
        words.push(TsvWord {
            block,
            paragraph,
            line,
            rect: Roi::new(x, y, width, height),
            confidence,
            text,
        });
    }
    words
}

/// Groups consecutive words that share block, paragraph and line numbers into one box.
#[must_use]
pub fn group_lines(words: &[TsvWord]) -> Vec<DetectedTextBox> {
    let mut lines = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let key = (
            words[start].block,
            words[start].paragraph,
            words[start].line,
        );
        let end = words[start..]
            .iter()
            .position(|w| (w.block, w.paragraph, w.line) != key)
            .map_or(words.len(), |offset| start + offset);
        lines.push(merge_words(&words[start..end]));
        start = end;
    }
    lines
}

fn merge_words(words: &[TsvWord]) -> DetectedTextBox {
    let tokens: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    let is_cjk = tokens.iter().any(|t| is_cjk_text(t));
    let text = join_cjk_or_latin_tokens(&tokens, is_cjk);

    let left = words.iter().map(|w| w.rect.x).min().unwrap_or(0);
    let top = words.iter().map(|w| w.rect.y).min().unwrap_or(0);
    let right = words
        .iter()
        .map(|w| w.rect.x.saturating_add(w.rect.width))
        .max()
        .unwrap_or(0);
    let bottom = words
        .iter()
        .map(|w| w.rect.y.saturating_add(w.rect.height))
        .max()
        .unwrap_or(0);
    let rect = Roi::new(left, top, right - left, bottom - top);

    DetectedTextBox::new(rect, text, mean_confidence(words) / 100.0)
}

/// Mean confidence weighted by character count, so one long word outweighs stray marks.
fn mean_confidence(words: &[TsvWord]) -> f32 {
    let (sum, chars) = words.iter().fold((0.0f32, 0usize), |(sum, chars), w| {
        let n = w.text.chars().count();
        (sum + w.confidence * n as f32, chars + n)
    });
    if chars == 0 {
        0.0
    } else {
        sum / chars as f32
    }
}

fn encode_png(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut cursor = std::io::Cursor::new(Vec::new());
    image
        .to_luma8()
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| anyhow!("Failed to encode image to PNG for Tesseract: {e}"))?;
    Ok(cursor.into_inner())
}

async fn run_tesseract_tsv(png: &[u8], lang: &str, psm: &str) -> Result<String> {
    let mut child = tokio::process::Command::new("tesseract")
        // Request TSV through -c instead of the `tsv` config name: that config file lives in
        // tessdata/configs and is absent from custom TESSDATA_PREFIX directories, in which
        // case Tesseract silently falls back to plain text.
        .args(["stdin", "stdout", "-l", lang, "--psm", psm])
        .args(["-c", "tessedit_create_tsv=1", "-c", "tessedit_create_txt=0"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn tesseract: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        // A write error usually means Tesseract exited early; its stderr explains why.
        if let Err(err) = stdin.write_all(png).await {
            warn!("Writing image to tesseract failed: {err}");
        }
    }

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let excerpt: String = stderr.trim().chars().take(STDERR_EXCERPT).collect();
        bail!(
            "tesseract -l {lang} exited with {}: {excerpt}",
            output.status
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if !is_tsv_output(&stdout) {
        let excerpt: String = stdout.trim().chars().take(STDERR_EXCERPT).collect();
        bail!("tesseract -l {lang} did not produce TSV output: {excerpt}");
    }
    Ok(stdout)
}

/// True for empty output or output starting with the TSV header row.
fn is_tsv_output(stdout: &str) -> bool {
    let trimmed = stdout.trim_start();
    trimmed.is_empty() || trimmed.starts_with("level\t")
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        "level\tpage_num\tblock_num\tpar_num\tline_num\tword_num\tleft\ttop\twidth\theight\tconf\ttext";

    fn tsv(rows: &[&str]) -> String {
        let mut s = String::from(HEADER);
        for r in rows {
            s.push('\n');
            s.push_str(r);
        }
        s
    }

    /// First line captured from tesseract 5.3.4 (`-l jpn --psm 6`) on a rendered dialogue
    /// line; the second line is hand-written in the same format.
    fn horizontal_fixture() -> String {
        tsv(&[
            "1\t1\t0\t0\t0\t0\t0\t0\t1000\t200\t-1\t",
            "2\t1\t1\t0\t0\t0\t27\t50\t881\t96\t-1\t",
            "3\t1\t1\t1\t0\t0\t27\t50\t881\t96\t-1\t",
            "4\t1\t1\t1\t1\t0\t27\t50\t881\t96\t-1\t",
            "5\t1\t1\t1\t1\t1\t27\t53\t178\t92\t96.026924\t明日",
            "5\t1\t1\t1\t1\t2\t277\t57\t39\t84\t96.358337\tは",
            "5\t1\t1\t1\t1\t3\t325\t50\t482\t96\t96.304527\t学校",
            "5\t1\t1\t1\t1\t4\t599\t46\t112\t133\t96.846611\tに",
            "4\t1\t1\t1\t2\t0\t29\t160\t300\t89\t-1\t",
            "5\t1\t1\t1\t2\t1\t29\t160\t145\t88\t92.929321\t行き",
            "5\t1\t1\t1\t2\t2\t180\t160\t100\t88\t92.0\tます",
            "5\t1\t1\t1\t2\t3\t280\t215\t32\t32\t92.874039\t。",
        ])
    }

    /// First column captured from tesseract 5.3.4 (`-l jpn_vert --psm 5`); the second
    /// column (to its left) is hand-written in the same format.
    fn vertical_fixture() -> String {
        tsv(&[
            "4\t1\t1\t1\t1\t0\t100\t27\t95\t585\t-1\t",
            "5\t1\t1\t1\t1\t1\t100\t27\t95\t585\t94.926712\t明日",
            "5\t1\t1\t1\t1\t2\t88\t281\t115\t73\t96.758232\tは",
            "5\t1\t1\t1\t1\t3\t100\t353\t95\t210\t96.780273\t学校",
            "4\t1\t1\t1\t2\t0\t0\t27\t95\t300\t-1\t",
            "5\t1\t1\t1\t2\t1\t0\t27\t95\t150\t95.0\t行き",
            "5\t1\t1\t1\t2\t2\t0\t180\t95\t147\t95.0\tます",
        ])
    }

    #[test]
    fn parse_keeps_only_word_rows() {
        let words = parse_tsv(&horizontal_fixture());
        assert_eq!(words.len(), 7);
        assert_eq!(words[0].text, "明日");
        assert_eq!(words[0].rect, Roi::new(27, 53, 178, 92));
        assert!((words[0].confidence - 96.026924).abs() < 1e-3);
        assert_eq!(words[4].line, 2);
    }

    #[test]
    fn parse_skips_malformed_and_empty_rows() {
        let input = tsv(&[
            "5\t1\t1\t1\t1\t1\tx\t0\t10\t10\t90\tbad",
            "5\t1\t1\t1\t1\t2\t0\t0\t10\t10\t90\t   ",
            "5\t1\t1",
            "5\t1\t1\t1\t1\t3\t0\t0\t10\t10\t-1\tok",
        ]);
        let words = parse_tsv(&input);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "ok");
        assert_eq!(words[0].confidence, 0.0);
    }

    #[test]
    fn group_lines_merges_rects_and_joins_cjk_without_spaces() {
        let lines = group_lines(&parse_tsv(&horizontal_fixture()));
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "明日は学校に");
        assert_eq!(lines[0].rect, Roi::new(27, 46, 780, 133));
        assert_eq!(lines[1].text, "行きます。");
    }

    #[test]
    fn group_lines_joins_latin_with_spaces() {
        let input = tsv(&[
            "5\t1\t1\t1\t1\t1\t0\t0\t50\t20\t90\tLevel",
            "5\t1\t1\t1\t1\t2\t60\t0\t20\t20\t90\t10",
        ]);
        let lines = group_lines(&parse_tsv(&input));
        assert_eq!(lines[0].text, "Level 10");
    }

    #[test]
    fn horizontal_result_is_joined_in_reading_order() {
        let lines = group_lines(&parse_tsv(&horizontal_fixture()));
        let result = assemble_result(lines, Some(TextOrientation::Horizontal), true);
        assert_eq!(result.text, "明日は学校に行きます。");
        assert_eq!(result.orientation, TextOrientation::Horizontal);
    }

    #[test]
    fn vertical_columns_are_read_right_to_left() {
        let lines = group_lines(&parse_tsv(&vertical_fixture()));
        assert_eq!(lines.len(), 2);
        let result = assemble_result(lines, Some(TextOrientation::Vertical), true);
        assert_eq!(result.text, "明日は学校行きます");
    }

    #[test]
    fn furigana_line_above_kanji_is_removed() {
        // Small ruby line "ゆうしゃ" directly above "勇者は剣を抜いた。".
        let input = tsv(&[
            "5\t1\t1\t1\t1\t1\t30\t10\t60\t18\t80\tゆうしゃ",
            "5\t1\t1\t1\t2\t1\t20\t32\t100\t50\t95\t勇者",
            "5\t1\t1\t1\t2\t2\t120\t32\t300\t50\t95\tは剣を抜いた。",
            "5\t1\t1\t1\t3\t1\t20\t100\t200\t50\t95\t次の行です",
        ]);
        let lines = group_lines(&parse_tsv(&input));
        assert_eq!(lines.len(), 3);
        let result = assemble_result(lines, Some(TextOrientation::Horizontal), true);
        assert_eq!(result.text, "勇者は剣を抜いた。次の行です");
    }

    #[test]
    fn stacked_single_characters_trigger_vertical_retry() {
        // Captured shape of `-l jpn --psm 6` on vertical text: one character per line.
        let rows: Vec<String> = ["明", "日", "は", "学", "に"]
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "5\t1\t1\t1\t{}\t1\t100\t{}\t95\t95\t93\t{c}",
                    i + 1,
                    i * 100
                )
            })
            .collect();
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let pass = Pass::new(TextOrientation::Horizontal, &parse_tsv(&tsv(&refs)));
        assert!(pass.confidence > AUTO_ACCEPT_CONFIDENCE);
        assert!(pass.is_doubtful());
    }

    #[test]
    fn low_confidence_wide_line_does_not_trigger_vertical_retry() {
        // Partially revealed horizontal line: low confidence, but clearly wide.
        let input = tsv(&["5\t1\t1\t1\t1\t1\t30\t40\t180\t50\t55\t明日は"]);
        let pass = Pass::new(TextOrientation::Horizontal, &parse_tsv(&input));
        assert!(pass.confidence < AUTO_ACCEPT_CONFIDENCE);
        assert!(!pass.is_doubtful());
    }

    #[test]
    fn low_confidence_tall_block_triggers_vertical_retry() {
        let input = tsv(&[
            "5\t1\t1\t1\t1\t1\t100\t0\t60\t200\t50\t明日",
            "5\t1\t1\t1\t2\t1\t100\t210\t60\t200\t50\t学校",
        ]);
        let pass = Pass::new(TextOrientation::Horizontal, &parse_tsv(&input));
        assert!(pass.is_doubtful());
    }

    #[test]
    fn confident_horizontal_pass_is_accepted() {
        let pass = Pass::new(
            TextOrientation::Horizontal,
            &parse_tsv(&horizontal_fixture()),
        );
        assert!(!pass.is_doubtful());
    }

    #[test]
    fn choose_pass_prefers_higher_confidence_and_horizontal_on_tie() {
        let h = Pass::new(
            TextOrientation::Horizontal,
            &parse_tsv(&horizontal_fixture()),
        );
        let mut v = Pass::new(TextOrientation::Vertical, &parse_tsv(&vertical_fixture()));
        v.confidence = h.confidence;
        assert_eq!(choose_pass(h, v).orientation, TextOrientation::Horizontal);

        let mut h = Pass::new(
            TextOrientation::Horizontal,
            &parse_tsv(&horizontal_fixture()),
        );
        h.confidence = 60.0;
        let v = Pass::new(TextOrientation::Vertical, &parse_tsv(&vertical_fixture()));
        assert_eq!(choose_pass(h, v).orientation, TextOrientation::Vertical);
    }

    #[test]
    fn mean_confidence_weights_by_characters() {
        let words = parse_tsv(&tsv(&[
            "5\t1\t1\t1\t1\t1\t0\t0\t10\t10\t90\tabcd",
            "5\t1\t1\t1\t1\t2\t0\t0\t10\t10\t40\t.",
        ]));
        assert!((mean_confidence(&words) - 80.0).abs() < 1e-3);
        assert_eq!(mean_confidence(&[]), 0.0);
    }

    #[test]
    fn plain_text_output_is_not_tsv() {
        assert!(is_tsv_output(&horizontal_fixture()));
        assert!(is_tsv_output(""));
        assert!(!is_tsv_output("明日は学校に行きます。\n"));
    }

    #[test]
    fn parse_lang_list() {
        let out = "List of available languages in \"/usr/share/tessdata/\" (3):\neng\njpn\nosd\n";
        assert_eq!(parse_tesseract_lang_list(out), vec!["eng", "jpn", "osd"]);
        assert!(parse_tesseract_lang_list("").is_empty());
    }
}
