use crate::region::Roi;
use serde::{Deserialize, Serialize};

/// Represents text layout orientation (horizontal or vertical CJK).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextOrientation {
    #[default]
    Horizontal,
    Vertical,
}

/// User-selected text direction for OCR engines that need it up front (Tesseract).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OrientationMode {
    /// Try horizontal first; fall back to vertical when the result looks wrong.
    #[default]
    Auto,
    Horizontal,
    Vertical,
}

impl OrientationMode {
    /// All modes, in settings dropdown order.
    pub const ALL: [Self; 3] = [Self::Auto, Self::Horizontal, Self::Vertical];

    /// Code stored in config.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
        }
    }

    /// Parses a stored code; unknown or empty values fall back to `Auto`.
    #[must_use]
    pub fn from_code(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "horizontal" | "yokogaki" => Self::Horizontal,
            "vertical" | "tategaki" => Self::Vertical,
            _ => Self::Auto,
        }
    }

    /// Fixed orientation, or `None` for automatic detection.
    #[must_use]
    pub fn forced(self) -> Option<TextOrientation> {
        match self {
            Self::Auto => None,
            Self::Horizontal => Some(TextOrientation::Horizontal),
            Self::Vertical => Some(TextOrientation::Vertical),
        }
    }

    /// Position in [`OrientationMode::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Auto => 0,
            Self::Horizontal => 1,
            Self::Vertical => 2,
        }
    }
}

/// A recognized text token/box with its geometric bounding rectangle and confidence score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectedTextBox {
    pub rect: Roi,
    pub text: String,
    pub confidence: f32,
}

impl DetectedTextBox {
    #[must_use]
    pub fn new(rect: Roi, text: impl Into<String>, confidence: f32) -> Self {
        Self {
            rect,
            text: text.into(),
            confidence,
        }
    }

    /// Aspect ratio defined as height / width.
    /// Values > 1.4 typically represent vertical text columns.
    /// Values < 0.8 typically represent horizontal text lines.
    #[must_use]
    pub fn aspect_ratio(&self) -> f32 {
        if self.rect.width == 0 {
            return 1.0;
        }
        self.rect.height as f32 / self.rect.width as f32
    }
}

/// Computes the 1D intersection span of two intervals along the vertical axis.
#[must_use]
pub(crate) fn vertical_overlap(a: &Roi, b: &Roi) -> u32 {
    let top = a.y.max(b.y);
    let bottom = (a.y.saturating_add(a.height)).min(b.y.saturating_add(b.height));
    bottom.saturating_sub(top)
}

/// Computes the 1D intersection span of two intervals along the horizontal axis.
#[must_use]
pub(crate) fn horizontal_overlap(a: &Roi, b: &Roi) -> u32 {
    let left = a.x.max(b.x);
    let right = (a.x.saturating_add(a.width)).min(b.x.saturating_add(b.width));
    right.saturating_sub(left)
}

/// Automatically detects the dominant text orientation of a set of bounding boxes.
/// Uses aspect ratio voting:
/// - $H / W > 1.4$: vertical orientation candidate (weighted by box height).
/// - $H / W < 0.8$: horizontal orientation candidate (weighted by box width).
///
/// Returns `TextOrientation::Vertical` if vertical weight exceeds horizontal weight;
/// otherwise returns `TextOrientation::Horizontal`.
#[must_use]
pub fn detect_dominant_orientation(boxes: &[DetectedTextBox]) -> TextOrientation {
    let mut vertical_weight = 0.0f32;
    let mut horizontal_weight = 0.0f32;

    for b in boxes {
        let r = b.aspect_ratio();
        if r > 1.4 {
            vertical_weight += b.rect.height as f32;
        } else if r < 0.8 {
            horizontal_weight += b.rect.width as f32;
        }
    }

    if vertical_weight > horizontal_weight {
        TextOrientation::Vertical
    } else {
        TextOrientation::Horizontal
    }
}

/// Topologically clusters and sorts bounding boxes into proper natural reading order.
///
/// - **Horizontal**: Clusters lines by vertical overlap $\ge 50\%$. Sorts lines top-to-bottom ($Y \uparrow$),
///   and tokens within each line left-to-right ($X \uparrow$).
/// - **Vertical**: Clusters columns by horizontal overlap $\ge 50\%$. Sorts columns right-to-left
///   ($X_{\text{right}} \downarrow$), and tokens within each column top-to-bottom ($Y \uparrow$).
#[must_use]
pub fn sort_cjk_reading_order(
    boxes: Vec<DetectedTextBox>,
    orientation: TextOrientation,
) -> Vec<DetectedTextBox> {
    if boxes.len() <= 1 {
        return boxes;
    }

    match orientation {
        TextOrientation::Horizontal => sort_horizontal_reading_order(boxes),
        TextOrientation::Vertical => sort_vertical_reading_order(boxes),
    }
}

fn sort_horizontal_reading_order(mut boxes: Vec<DetectedTextBox>) -> Vec<DetectedTextBox> {
    // Sort initially by Y ascending, then X ascending
    boxes.sort_by(|a, b| {
        a.rect
            .y
            .cmp(&b.rect.y)
            .then_with(|| a.rect.x.cmp(&b.rect.x))
            .then_with(|| a.rect.height.cmp(&b.rect.height))
            .then_with(|| a.rect.width.cmp(&b.rect.width))
            .then_with(|| a.text.cmp(&b.text))
    });
    let mut lines: Vec<Vec<DetectedTextBox>> = Vec::new();

    for b in boxes {
        let mut placed = false;
        for line in &mut lines {
            if let Some(first) = line.first() {
                let overlap = vertical_overlap(&first.rect, &b.rect);
                let min_h = first.rect.height.min(b.rect.height) as f32;
                if min_h > 0.0 && (overlap as f32 / min_h) >= 0.5 {
                    line.push(b.clone());
                    placed = true;
                    break;
                }
            }
        }
        if !placed {
            lines.push(vec![b]);
        }
    }

    // Sort lines top-to-bottom (Y asc)
    lines.sort_by(|line_a, line_b| {
        let a_top = line_a.iter().map(|b| b.rect.y).min().unwrap_or(0);
        let b_top = line_b.iter().map(|b| b.rect.y).min().unwrap_or(0);
        a_top.cmp(&b_top).then_with(|| {
            let a_left = line_a.iter().map(|b| b.rect.x).min().unwrap_or(0);
            let b_left = line_b.iter().map(|b| b.rect.x).min().unwrap_or(0);
            a_left.cmp(&b_left)
        })
    });

    // Sort tokens within line left-to-right (X asc)
    for line in &mut lines {
        line.sort_by(|a, b| {
            a.rect
                .x
                .cmp(&b.rect.x)
                .then_with(|| a.rect.y.cmp(&b.rect.y))
                .then_with(|| a.rect.width.cmp(&b.rect.width))
                .then_with(|| a.rect.height.cmp(&b.rect.height))
                .then_with(|| a.text.cmp(&b.text))
        });
    }

    lines.into_iter().flatten().collect()
}

fn sort_vertical_reading_order(mut boxes: Vec<DetectedTextBox>) -> Vec<DetectedTextBox> {
    // For vertical columns: sort initially by right edge descending (X_right desc)
    boxes.sort_by(|a, b| {
        let a_right = a.rect.x.saturating_add(a.rect.width);
        let b_right = b.rect.x.saturating_add(b.rect.width);
        b_right
            .cmp(&a_right)
            .then_with(|| a.rect.y.cmp(&b.rect.y))
            .then_with(|| a.rect.x.cmp(&b.rect.x))
            .then_with(|| a.rect.height.cmp(&b.rect.height))
            .then_with(|| a.rect.width.cmp(&b.rect.width))
            .then_with(|| a.text.cmp(&b.text))
    });

    let mut columns: Vec<Vec<DetectedTextBox>> = Vec::new();

    for b in boxes {
        let mut placed = false;
        for col in &mut columns {
            if let Some(first) = col.first() {
                let overlap = horizontal_overlap(&first.rect, &b.rect);
                let min_w = first.rect.width.min(b.rect.width) as f32;
                if min_w > 0.0 && (overlap as f32 / min_w) >= 0.5 {
                    col.push(b.clone());
                    placed = true;
                    break;
                }
            }
        }
        if !placed {
            columns.push(vec![b]);
        }
    }

    // Sort columns right-to-left (X_right desc)
    columns.sort_by(|col_a, col_b| {
        let a_right = col_a
            .iter()
            .map(|b| b.rect.x.saturating_add(b.rect.width))
            .max()
            .unwrap_or(0);
        let b_right = col_b
            .iter()
            .map(|b| b.rect.x.saturating_add(b.rect.width))
            .max()
            .unwrap_or(0);
        b_right.cmp(&a_right).then_with(|| {
            let a_top = col_a.iter().map(|b| b.rect.y).min().unwrap_or(0);
            let b_top = col_b.iter().map(|b| b.rect.y).min().unwrap_or(0);
            a_top.cmp(&b_top).then_with(|| {
                let a_left = col_a.iter().map(|b| b.rect.x).min().unwrap_or(0);
                let b_left = col_b.iter().map(|b| b.rect.x).min().unwrap_or(0);
                b_left.cmp(&a_left)
            })
        })
    });

    // Sort tokens within column top-to-bottom (Y asc)
    for col in &mut columns {
        col.sort_by(|a, b| {
            a.rect
                .y
                .cmp(&b.rect.y)
                .then_with(|| a.rect.x.cmp(&b.rect.x))
                .then_with(|| a.rect.height.cmp(&b.rect.height))
                .then_with(|| a.rect.width.cmp(&b.rect.width))
                .then_with(|| a.text.cmp(&b.text))
        });
    }

    columns.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aspect_ratio_classification() {
        // Vertical column: W=20, H=80 -> aspect ratio 4.0 > 1.4
        let vert_box = DetectedTextBox::new(Roi::new(10, 10, 20, 80), "縦書き", 0.95);
        assert!(vert_box.aspect_ratio() > 1.4);

        // Horizontal line: W=100, H=25 -> aspect ratio 0.25 < 0.8
        let horiz_box = DetectedTextBox::new(Roi::new(10, 10, 100, 25), "横書き", 0.95);
        assert!(horiz_box.aspect_ratio() < 0.8);

        // Square character: W=30, H=30 -> aspect ratio 1.0 (ambiguous)
        let square_box = DetectedTextBox::new(Roi::new(10, 10, 30, 30), "字", 0.95);
        assert!((0.8..=1.4).contains(&square_box.aspect_ratio()));
    }

    #[test]
    fn test_dominant_orientation_voting() {
        // 2 tall vertical columns vs 1 short horizontal line
        let boxes = vec![
            DetectedTextBox::new(Roi::new(100, 20, 20, 100), "列一", 0.9),
            DetectedTextBox::new(Roi::new(60, 20, 20, 100), "列二", 0.9),
            DetectedTextBox::new(Roi::new(10, 200, 50, 15), "横", 0.9),
        ];
        assert_eq!(
            detect_dominant_orientation(&boxes),
            TextOrientation::Vertical
        );

        // 3 wide horizontal lines
        let boxes_horiz = vec![
            DetectedTextBox::new(Roi::new(10, 20, 120, 25), "Line 1", 0.9),
            DetectedTextBox::new(Roi::new(10, 50, 120, 25), "Line 2", 0.9),
            DetectedTextBox::new(Roi::new(10, 80, 120, 25), "Line 3", 0.9),
        ];
        assert_eq!(
            detect_dominant_orientation(&boxes_horiz),
            TextOrientation::Horizontal
        );

        // Empty boxes default to horizontal
        assert_eq!(
            detect_dominant_orientation(&[]),
            TextOrientation::Horizontal
        );
    }

    #[test]
    fn test_horizontal_reading_order_top_to_bottom() {
        // Line 2 (Y=50) tokens scrambled, Line 1 (Y=20) tokens scrambled
        let boxes = vec![
            DetectedTextBox::new(Roi::new(80, 50, 30, 20), "line2_word2", 0.9),
            DetectedTextBox::new(Roi::new(10, 20, 30, 20), "line1_word1", 0.9),
            DetectedTextBox::new(Roi::new(50, 20, 30, 20), "line1_word2", 0.9),
            DetectedTextBox::new(Roi::new(10, 50, 30, 20), "line2_word1", 0.9),
        ];

        let sorted = sort_cjk_reading_order(boxes, TextOrientation::Horizontal);
        let texts: Vec<&str> = sorted.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["line1_word1", "line1_word2", "line2_word1", "line2_word2"]
        );
    }

    #[test]
    fn test_vertical_reading_order_right_to_left() {
        // Two vertical columns in Japanese tategaki:
        // Col 1 (Right): X=100..120, Y=10, Y=40, Y=70
        // Col 2 (Left):  X=50..70,   Y=10, Y=40
        // Shuffled input order
        let boxes = vec![
            DetectedTextBox::new(Roi::new(50, 40, 20, 25), "左_下", 0.9),
            DetectedTextBox::new(Roi::new(100, 70, 20, 25), "右_下", 0.9),
            DetectedTextBox::new(Roi::new(100, 10, 20, 25), "右_上", 0.9),
            DetectedTextBox::new(Roi::new(50, 10, 20, 25), "左_上", 0.9),
            DetectedTextBox::new(Roi::new(100, 40, 20, 25), "右_中", 0.9),
        ];

        let sorted = sort_cjk_reading_order(boxes, TextOrientation::Vertical);
        let texts: Vec<&str> = sorted.iter().map(|b| b.text.as_str()).collect();

        // Right column read top-to-bottom, then left column read top-to-bottom
        assert_eq!(texts, vec!["右_上", "右_中", "右_下", "左_上", "左_下"]);
    }
}
