//! PaddleOCR (PP-OCR) detection and recognition models executed with `rten`.
//!
//! Pipeline:
//!
//! 1. Detection (DB): the image is resized so the longer side is at most
//!    [`DET_LIMIT_SIDE`] (both sides multiples of 32), normalised with ImageNet
//!    statistics in BGR order and run through the model, which outputs a text
//!    probability map. Connected regions above [`DB_THRESHOLD`] with a mean score of at
//!    least [`DB_BOX_THRESHOLD`] become axis-aligned boxes, expanded by the DB unclip
//!    distance and mapped back to image coordinates.
//! 2. Recognition (SVTR/CTC): each box is cropped, rotated 90 degrees counter-clockwise if
//!    it is a vertical column, resized to a height of [`REC_HEIGHT`] and decoded with
//!    greedy CTC against the model's character dictionary.
//!
//! Boxes are axis-aligned: game dialogue boxes and subtitles are not rotated, so the
//! minimum-area rectangle step of the reference implementation is omitted.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use image::{imageops, DynamicImage, RgbImage};
use rten::Model;
use rten_tensor::prelude::*;
use rten_tensor::{NdTensor, Tensor};

use super::models::{self, ModelSource};
use super::{
    assemble_result, BoxFuture, DetectedTextBox, OcrBackend, OcrResult, OrientationMode,
    TextOrientation,
};
use crate::capture::roi::Roi;
use crate::error::OcrInitError;
use crate::language::Script;

/// Download URL of a file in the `models-v1` release of this project.
macro_rules! release_url {
    ($file:literal) => {
        concat!(
            "https://github.com/Hasmolam/cosmic-ext-translator/releases/download/models-v1/",
            $file
        )
    };
}

/// Longer image side limit for detection input.
pub const DET_LIMIT_SIDE: u32 = 960;
/// Shorter side lower bound for detection input; small ROIs are upscaled to this.
pub const DET_MIN_SIDE: u32 = 64;
/// Probability above which a pixel of the DB map counts as text.
pub const DB_THRESHOLD: f32 = 0.3;
/// Minimum mean probability of a region for it to be kept.
pub const DB_BOX_THRESHOLD: f32 = 0.6;
/// DB unclip ratio: boxes grow by `area * ratio / perimeter` on each side.
pub const DB_UNCLIP_RATIO: f32 = 1.5;
/// Recognition input height.
pub const REC_HEIGHT: u32 = 48;
/// Minimum recognition input width (PP-OCR pads to a 320x48 canvas).
pub const REC_MIN_WIDTH: u32 = 320;
/// Crops with `height / width` at or above this ratio are treated as vertical columns.
pub const VERTICAL_RATIO: f32 = 1.5;

const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];

/// CTC output labels: index 0 is the blank symbol.
#[derive(Clone, PartialEq, Eq)]
pub struct CtcLabels {
    labels: Vec<String>,
}

impl std::fmt::Debug for CtcLabels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CtcLabels")
            .field("classes", &self.labels.len())
            .finish()
    }
}

impl CtcLabels {
    /// Builds the label table for a model with `classes` output classes from a PaddleOCR
    /// dictionary file (one character per line).
    ///
    /// Published dictionaries differ in whether they already contain the blank placeholder
    /// and the trailing space class, so the layout is inferred from the line count:
    ///
    /// - `classes - 2` lines: blank + lines + space (reference `use_space_char` layout).
    /// - `classes - 1` lines starting with an empty line: that line is the blank
    ///   placeholder and the space class follows the last line.
    /// - `classes - 1` lines otherwise: blank + lines.
    /// - `classes` lines: the first line is the blank placeholder.
    pub fn from_dict(dict: &str, classes: usize) -> Result<Self> {
        let mut lines: Vec<&str> = dict.split('\n').collect();
        if lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        let count = lines.len();
        let first_is_empty = lines
            .first()
            .is_some_and(|l| l.trim_end_matches('\r').is_empty());
        let lines = lines.into_iter().map(|l| l.strip_suffix('\r').unwrap_or(l));

        let mut labels: Vec<String> = Vec::with_capacity(classes);
        if count + 2 == classes {
            labels.push(String::new());
            labels.extend(lines.map(str::to_string));
            labels.push(" ".to_string());
        } else if count + 1 == classes && first_is_empty {
            // An empty first line is the blank placeholder; the space class is implicit.
            labels.extend(lines.map(str::to_string));
            labels.push(" ".to_string());
        } else if count + 1 == classes {
            labels.push(String::new());
            labels.extend(lines.map(str::to_string));
        } else if count == classes {
            labels.extend(lines.map(str::to_string));
            if let Some(first) = labels.first_mut() {
                first.clear();
            }
        } else {
            return Err(anyhow!(
                "dictionary has {count} entries, model has {classes} classes"
            ));
        }
        Ok(Self { labels })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    /// Greedy CTC decoding of a `[time, classes]` score matrix in row-major order.
    ///
    /// Returns the text and the mean score of the emitted characters. Scores are expected
    /// to be probabilities (the exported PP-OCR models end in a softmax).
    #[must_use]
    pub fn decode(&self, scores: &[f32], classes: usize) -> (String, f32) {
        let mut text = String::new();
        let mut score_sum = 0.0f32;
        let mut emitted = 0usize;
        let mut prev = 0usize;
        if classes == 0 {
            return (text, 0.0);
        }
        for step in scores.chunks_exact(classes) {
            let (best, best_score) =
                step.iter()
                    .copied()
                    .enumerate()
                    .fold(
                        (0usize, f32::MIN),
                        |acc, (i, s)| if s > acc.1 { (i, s) } else { acc },
                    );
            if best != 0 && best != prev {
                if let Some(label) = self.labels.get(best) {
                    text.push_str(label);
                    score_sum += best_score;
                    emitted += 1;
                }
            }
            prev = best;
        }
        let confidence = if emitted == 0 {
            0.0
        } else {
            score_sum / emitted as f32
        };
        (text, confidence)
    }
}

/// Axis-aligned box in image pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// Size of the detection input for an image of `width` x `height`.
#[must_use]
pub fn det_input_size(width: u32, height: u32) -> (u32, u32) {
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let mut scale = 1.0f32;
    if w.max(h) > DET_LIMIT_SIDE as f32 {
        scale = DET_LIMIT_SIDE as f32 / w.max(h);
    }
    if w.min(h) * scale < DET_MIN_SIDE as f32 {
        scale = (DET_MIN_SIDE as f32 / w.min(h)).min(DET_LIMIT_SIDE as f32 / w.max(h));
    }
    let round32 = |v: f32| (((v / 32.0).round() as u32) * 32).max(32);
    (round32(w * scale), round32(h * scale))
}

fn det_input(image: &RgbImage) -> NdTensor<f32, 4> {
    let (w, h) = det_input_size(image.width(), image.height());
    let resized = imageops::resize(image, w, h, imageops::FilterType::Triangle);
    let plane = (w * h) as usize;
    let mut data = vec![0.0f32; 3 * plane];
    for (i, px) in resized.pixels().enumerate() {
        // BGR channel order, as PaddleOCR reads images with OpenCV.
        for (c, value) in [px[2], px[1], px[0]].into_iter().enumerate() {
            data[c * plane + i] = (f32::from(value) / 255.0 - DET_MEAN[c]) / DET_STD[c];
        }
    }
    NdTensor::from_data([1, 3, h as usize, w as usize], data)
}

/// Extracts text boxes from a DB probability map of `width` x `height`.
#[must_use]
pub fn boxes_from_prob_map(prob: &[f32], width: usize, height: usize) -> Vec<PixelBox> {
    let mut visited = vec![false; prob.len()];
    let mut boxes = Vec::new();
    let mut queue = VecDeque::new();

    for start in 0..prob.len().min(width * height) {
        if visited[start] || prob[start] <= DB_THRESHOLD {
            continue;
        }
        visited[start] = true;
        queue.push_back(start);
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        let mut pixels = 0usize;
        while let Some(idx) = queue.pop_front() {
            let (x, y) = (idx % width, idx / width);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            pixels += 1;
            let neighbours = [
                (x > 0).then(|| idx - 1),
                (x + 1 < width).then(|| idx + 1),
                (y > 0).then(|| idx - width),
                (y + 1 < height).then(|| idx + width),
            ];
            for n in neighbours.into_iter().flatten() {
                if !visited[n] && prob[n] > DB_THRESHOLD {
                    visited[n] = true;
                    queue.push_back(n);
                }
            }
        }

        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        if bw.min(bh) < 3 || pixels < 10 {
            continue;
        }
        let mut sum = 0.0f32;
        for y in y0..=y1 {
            sum += prob[y * width + x0..=y * width + x1].iter().sum::<f32>();
        }
        if sum / ((bw * bh) as f32) < DB_BOX_THRESHOLD {
            continue;
        }

        let (fw, fh) = (bw as f32, bh as f32);
        let distance = fw * fh * DB_UNCLIP_RATIO / (2.0 * (fw + fh));
        boxes.push(PixelBox {
            x0: (x0 as f32 - distance).max(0.0),
            y0: (y0 as f32 - distance).max(0.0),
            x1: (x1 as f32 + 1.0 + distance).min(width as f32),
            y1: (y1 as f32 + 1.0 + distance).min(height as f32),
        });
    }
    boxes
}

fn rec_input(crop: &RgbImage) -> NdTensor<f32, 4> {
    let ratio = crop.width().max(1) as f32 / crop.height().max(1) as f32;
    let w = ((REC_HEIGHT as f32 * ratio).ceil() as u32).max(1);
    let canvas_w = w.max(REC_MIN_WIDTH) as usize;
    let resized = imageops::resize(crop, w, REC_HEIGHT, imageops::FilterType::Triangle);
    let h = REC_HEIGHT as usize;
    let plane = h * canvas_w;
    // Padding stays 0, which is mid-grey after normalisation, as in the reference.
    let mut data = vec![0.0f32; 3 * plane];
    for (x, y, px) in resized.enumerate_pixels() {
        let i = y as usize * canvas_w + x as usize;
        for (c, value) in [px[2], px[1], px[0]].into_iter().enumerate() {
            data[c * plane + i] = f32::from(value) / 127.5 - 1.0;
        }
    }
    NdTensor::from_data([1, 3, h, canvas_w], data)
}

fn run_f32(model: &Model, input: NdTensor<f32, 4>) -> Result<Tensor<f32>> {
    let output = model
        .run_one(input.view().into(), None)
        .map_err(|e| anyhow!("model inference failed: {e}"))?;
    output
        .try_into()
        .map_err(|e| anyhow!("unexpected model output type: {e:?}"))
}

/// PP-OCRv5 mobile detection model (Apache-2.0, PaddlePaddle).
pub const PPOCRV5_DET: ModelSource = ModelSource {
    file_name: "ppocrv5-mobile-det.onnx",
    url: release_url!("ppocrv5-mobile-det.onnx"),
    legacy_names: &[],
    sha256: Some("d7fe3ea74652890722c0f4d02458b7261d9f5ae6c92904d05707c9eb155c7924"),
};

/// PP-OCRv5 mobile multilingual recognition model: Chinese, Japanese, English.
pub const PPOCRV5_REC: ModelSource = ModelSource {
    file_name: "ppocrv5-mobile-rec.onnx",
    url: release_url!("ppocrv5-mobile-rec.onnx"),
    legacy_names: &[],
    sha256: Some("d253c3cbee6e507828a5271a30ab0ec8ae7c2a99d0cc8e6f844fe380809d22b3"),
};

/// Character dictionary for [`PPOCRV5_REC`].
pub const PPOCRV5_DICT: ModelSource = ModelSource {
    file_name: "ppocrv5-dict.txt",
    url: release_url!("ppocrv5-dict.txt"),
    legacy_names: &[],
    sha256: Some("9dfc80c50b6cb07399a47a7cf25d11db475fb4ad0e1fc96b2eff6467c8166ff3"),
};

/// Sub-directory of the models directory that holds the PP-OCRv5 set.
pub const PPOCRV5_SET: &str = "ppocrv5";

/// Returns true if the PP-OCRv5 recogniser covers `script`.
#[must_use]
pub fn supports_script(script: Script) -> bool {
    matches!(script, Script::Japanese | Script::ChineseSimplified)
}

/// PaddleOCR detector + recogniser pair.
#[derive(Debug, Clone)]
pub struct PaddleOcrBackend {
    det: Arc<Model>,
    rec: Arc<Model>,
    labels: Arc<CtcLabels>,
    mode: OrientationMode,
}

impl PaddleOcrBackend {
    /// Builds the backend from loaded models. Runs one recognition pass to read the class
    /// count, so call it from a blocking context.
    pub fn from_models(det: Model, rec: Model, dict_text: &str) -> Result<Self> {
        let probe = run_f32(&rec, rec_input(&RgbImage::new(REC_MIN_WIDTH, REC_HEIGHT)))?;
        let classes = probe
            .shape()
            .last()
            .copied()
            .ok_or_else(|| anyhow!("recognition output has no class dimension"))?;
        let labels = CtcLabels::from_dict(dict_text, classes)?;

        Ok(Self {
            det: Arc::new(det),
            rec: Arc::new(rec),
            labels: Arc::new(labels),
            mode: OrientationMode::Auto,
        })
    }

    /// Loads the models and dictionary from explicit paths. Blocking.
    pub fn load_blocking(det: &Path, rec: &Path, dict: &Path) -> Result<Self> {
        let det_model =
            Model::load_file(det).with_context(|| format!("loading detection model {det:?}"))?;
        let rec_model =
            Model::load_file(rec).with_context(|| format!("loading recognition model {rec:?}"))?;
        let dict_text = std::fs::read_to_string(dict)
            .with_context(|| format!("reading dictionary {dict:?}"))?;
        Self::from_models(det_model, rec_model, &dict_text)
    }

    /// Loads the PP-OCRv5 set from `/usr/share/cosmic-ext-translator/models/ppocrv5` if
    /// complete, otherwise from the user models directory, downloading missing files.
    pub async fn load_default_or_download() -> std::result::Result<Self, OcrInitError> {
        let system_dir = Path::new(models::SYSTEM_MODELS_DIR).join(PPOCRV5_SET);
        let system_files = [PPOCRV5_DET, PPOCRV5_REC, PPOCRV5_DICT]
            .map(|source| models::find_existing(&system_dir, &source));

        let (det, rec, dict_path) = if let [Some(det), Some(rec), Some(dict)] = system_files {
            tracing::info!("Loading PaddleOCR models from {system_dir:?}");
            (
                models::load_readonly(&det).await?,
                models::load_readonly(&rec).await?,
                dict,
            )
        } else {
            let dir = models::user_models_dir().join(PPOCRV5_SET);
            (
                models::ensure_model(&dir, PPOCRV5_DET, None).await?,
                models::ensure_model(&dir, PPOCRV5_REC, None).await?,
                models::ensure_file(&dir, PPOCRV5_DICT).await?,
            )
        };

        let dict_text =
            tokio::fs::read_to_string(&dict_path)
                .await
                .map_err(|e| OcrInitError::ModelLoad {
                    path: dict_path.clone(),
                    reason: e.to_string(),
                })?;
        tokio::task::spawn_blocking(move || {
            Self::from_models(det, rec, &dict_text)
                .map_err(|e| OcrInitError::Engine(format!("PaddleOCR: {e:#}")))
        })
        .await
        .map_err(|e| OcrInitError::Task(e.to_string()))?
    }

    /// Forces the reported text orientation instead of inferring it from box shapes.
    #[must_use]
    pub fn with_mode(mut self, mode: OrientationMode) -> Self {
        self.mode = mode;
        self
    }

    /// Detects and recognises text lines. Blocking; runs inference on the calling thread.
    pub fn detect_and_recognize(&self, image: &RgbImage) -> Result<Vec<DetectedTextBox>> {
        let prob = run_f32(&self.det, det_input(image))?;
        let shape = prob.shape().to_vec();
        let (map_h, map_w) = match shape.as_slice() {
            [.., h, w] => (*h, *w),
            _ => return Err(anyhow!("unexpected detection output shape {shape:?}")),
        };
        let prob = prob.to_vec();
        // The map matches the resized input; scale map coordinates back to the image.
        let sx = image.width() as f32 / map_w as f32;
        let sy = image.height() as f32 / map_h as f32;

        let mut results = Vec::new();
        for b in boxes_from_prob_map(&prob[..map_w * map_h], map_w, map_h) {
            let x0 = (b.x0 * sx).floor().max(0.0) as u32;
            let y0 = (b.y0 * sy).floor().max(0.0) as u32;
            let x1 = ((b.x1 * sx).ceil() as u32).min(image.width());
            let y1 = ((b.y1 * sy).ceil() as u32).min(image.height());
            if x1 <= x0 + 1 || y1 <= y0 + 1 {
                continue;
            }
            let (w, h) = (x1 - x0, y1 - y0);
            let crop = imageops::crop_imm(image, x0, y0, w, h).to_image();
            let vertical = h as f32 / w as f32 >= VERTICAL_RATIO;
            let crop = if vertical {
                imageops::rotate270(&crop)
            } else {
                crop
            };

            let scores = run_f32(&self.rec, rec_input(&crop))?;
            let classes = scores.shape().last().copied().unwrap_or(0);
            let (text, confidence) = self.labels.decode(&scores.to_vec(), classes);
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let rect = Roi {
                x: x0,
                y: y0,
                width: w,
                height: h,
            };
            results.push(DetectedTextBox::new(rect, text, confidence));
        }
        Ok(results)
    }
}

impl OcrBackend for PaddleOcrBackend {
    fn recognize<'a>(&'a self, image: &'a DynamicImage) -> BoxFuture<'a, Result<String>> {
        Box::pin(async move { Ok(self.recognize_detailed(image, None).await?.text) })
    }

    fn recognize_detailed<'a>(
        &'a self,
        image: &'a DynamicImage,
        orientation: Option<TextOrientation>,
    ) -> BoxFuture<'a, Result<OcrResult>> {
        let rgb = image.to_rgb8();
        let backend = self.clone();
        let orientation = orientation.or(self.mode.forced());
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let boxes = backend.detect_and_recognize(&rgb)?;
                Ok(assemble_result(boxes, orientation, true))
            })
            .await
            .map_err(|e| anyhow!("Spawn blocking error: {e}"))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dict_layouts_are_inferred_from_class_count() {
        let dict = "a\nb\n";
        let with_space = CtcLabels::from_dict(dict, 4).unwrap();
        assert_eq!(with_space.labels, ["", "a", "b", " "]);
        let plain = CtcLabels::from_dict(dict, 3).unwrap();
        assert_eq!(plain.labels, ["", "a", "b"]);
        let leading_blank = CtcLabels::from_dict("\na\nb\n", 4).unwrap();
        assert_eq!(leading_blank.labels, ["", "a", "b", " "]);
        let with_blank = CtcLabels::from_dict("<blank>\na\nb", 3).unwrap();
        assert_eq!(with_blank.labels, ["", "a", "b"]);
        assert!(CtcLabels::from_dict(dict, 10).is_err());
    }

    #[test]
    fn ctc_collapses_repeats_and_drops_blanks() {
        let labels = CtcLabels::from_dict("あ\nい", 3).unwrap();
        // Steps: あ あ blank あ い い blank
        let steps = [1, 1, 0, 1, 2, 2, 0];
        let mut scores = Vec::new();
        for s in steps {
            let mut row = [0.05f32; 3];
            row[s] = 0.9;
            scores.extend_from_slice(&row);
        }
        let (text, conf) = labels.decode(&scores, 3);
        assert_eq!(text, "ああい");
        assert!((conf - 0.9).abs() < 1e-6);
    }

    #[test]
    fn model_checksums_match_release_manifest() {
        let manifest = include_str!("../../../../packaging/models/SHA256SUMS");
        for source in [PPOCRV5_DET, PPOCRV5_REC, PPOCRV5_DICT] {
            let line = manifest
                .lines()
                .find(|l| l.ends_with(source.file_name))
                .unwrap_or_else(|| panic!("{} missing from SHA256SUMS", source.file_name));
            assert_eq!(Some(&line[..64]), source.sha256, "{}", source.file_name);
            assert!(source.url.ends_with(source.file_name));
        }
    }

    #[test]
    fn det_input_size_respects_limits() {
        assert_eq!(det_input_size(1920, 300), (960, 160));
        assert_eq!(det_input_size(900, 260), (896, 256));
        let (w, h) = det_input_size(400, 30);
        assert!(h >= 64 && w % 32 == 0 && h % 32 == 0, "{w}x{h}");
    }

    #[test]
    fn prob_map_regions_become_unclipped_boxes() {
        let (w, h) = (40, 20);
        let mut prob = vec![0.0f32; w * h];
        for y in 5..10 {
            for x in 4..30 {
                prob[y * w + x] = 0.9;
            }
        }
        // A weak region below the box threshold is ignored.
        for y in 14..18 {
            for x in 4..30 {
                prob[y * w + x] = 0.4;
            }
        }
        let boxes = boxes_from_prob_map(&prob, w, h);
        assert_eq!(boxes.len(), 1);
        let b = boxes[0];
        assert!(
            b.x0 < 4.0 && b.y0 < 5.0 && b.x1 > 30.0 && b.y1 > 10.0,
            "{b:?}"
        );
    }
}
