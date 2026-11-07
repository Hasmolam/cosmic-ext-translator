use crate::layout::{horizontal_overlap, vertical_overlap, DetectedTextBox, TextOrientation};
use translator_protocol::Roi;

/// Returns true if a character is a recognized punctuation symbol that must be protected
/// from furigana / ruby text pruning.
#[must_use]
pub fn is_punctuation_char(c: char) -> bool {
    matches!(
        c,
        '。' | '、' | '！' | '？' | '…' | '・' | '.' | ',' | '!' | '?'
    )
}

/// Filters out undersized ruby (furigana) annotations positioned adjacent to primary text.
///
/// 1. Computes median area, median width, and median height across all bounding boxes.
/// 2. Identifies primary text boxes with $\text{area} \ge 0.35 \times \text{median\_area}$.
/// 3. Protects punctuation characters (`。`, `、`, `！`, `？`, `…`, `・`, `.`, `,`, `!`, `?`)
///    from being pruned even if their bounding boxes are small.
/// 4. Filters out candidate ruby boxes with $\text{area} < 0.35 \times \text{median\_area}$
///    that are positioned in the ruby lane adjacent to a primary text line (above) or column (to the right).
#[must_use]
pub fn filter_furigana(
    boxes: Vec<DetectedTextBox>,
    orientation: TextOrientation,
) -> Vec<DetectedTextBox> {
    if boxes.len() < 2 {
        return boxes;
    }

    let mut areas: Vec<u64> = boxes
        .iter()
        .map(|b| (b.rect.width as u64) * (b.rect.height as u64))
        .collect();
    areas.sort_unstable();
    let median_area = areas[areas.len() / 2];

    let mut widths: Vec<u32> = boxes.iter().map(|b| b.rect.width).collect();
    widths.sort_unstable();
    let median_width = widths[widths.len() / 2];

    let mut heights: Vec<u32> = boxes.iter().map(|b| b.rect.height).collect();
    heights.sort_unstable();
    let median_height = heights[heights.len() / 2];

    let area_threshold = (median_area as f64 * 0.35) as u64;

    let primary_rects: Vec<Roi> = boxes
        .iter()
        .filter_map(|b| {
            let area = (b.rect.width as u64) * (b.rect.height as u64);
            if area >= area_threshold {
                Some(b.rect)
            } else {
                None
            }
        })
        .collect();

    if primary_rects.is_empty() {
        return boxes;
    }

    boxes
        .into_iter()
        .filter(|b| {
            let area = (b.rect.width as u64) * (b.rect.height as u64);
            if area >= area_threshold {
                return true;
            }

            // Punctuation check: protect small punctuation marks
            let trimmed = b.text.trim();
            if !trimmed.is_empty() && trimmed.chars().all(is_punctuation_char) {
                return true;
            }

            // Spatial adjacency check: is this box in the ruby lane of any primary text block?
            let is_adjacent_ruby = match orientation {
                TextOrientation::Vertical => {
                    // In vertical text, furigana sits to the right flank of the primary column
                    primary_rects.iter().any(|p_rect| {
                        let vert_overlap = vertical_overlap(&b.rect, p_rect);
                        if vert_overlap == 0 {
                            return false;
                        }

                        let p_center_x = p_rect.x.saturating_add(p_rect.width / 2);
                        let b_center_x = b.rect.x.saturating_add(b.rect.width / 2);
                        if b_center_x < p_center_x {
                            return false;
                        }

                        let p_right = p_rect.x.saturating_add(p_rect.width);
                        let dx = (b.rect.x as i64) - (p_right as i64);
                        let max_gap = (0.8 * median_width as f64).max(10.0) as i64;
                        let min_gap = -((p_rect.width as i64) / 3);

                        dx >= min_gap && dx <= max_gap
                    })
                }
                TextOrientation::Horizontal => {
                    // In horizontal text, furigana sits above the primary text line
                    primary_rects.iter().any(|p_rect| {
                        let horiz_overlap = horizontal_overlap(&b.rect, p_rect);
                        if horiz_overlap == 0 {
                            return false;
                        }

                        let p_center_y = p_rect.y.saturating_add(p_rect.height / 2);
                        let b_center_y = b.rect.y.saturating_add(b.rect.height / 2);
                        if b_center_y > p_center_y {
                            return false;
                        }

                        let p_top = p_rect.y as i64;
                        let b_bottom = (b.rect.y.saturating_add(b.rect.height)) as i64;
                        let dy = p_top - b_bottom;
                        let max_gap = (0.8 * median_height as f64).max(10.0) as i64;
                        let min_gap = -((p_rect.height as i64) / 3);

                        dy >= min_gap && dy <= max_gap
                    })
                }
            };

            // If it is adjacent ruby text, prune it (return false); otherwise keep it
            !is_adjacent_ruby
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use translator_protocol::Roi;

    #[test]
    fn test_punctuation_protection() {
        assert!(is_punctuation_char('。'));
        assert!(is_punctuation_char('、'));
        assert!(is_punctuation_char('！'));
        assert!(is_punctuation_char('？'));
        assert!(is_punctuation_char('…'));
        assert!(is_punctuation_char('・'));
        assert!(is_punctuation_char('.'));
        assert!(is_punctuation_char(','));
        assert!(is_punctuation_char('!'));
        assert!(is_punctuation_char('?'));
        assert!(!is_punctuation_char('あ'));
        assert!(!is_punctuation_char('A'));
    }

    #[test]
    fn test_vertical_furigana_pruning_with_punctuation_retained() {
        // Main kanji column 1: "日本語" (X=100..140, Y=50..170, W=40, H=120, area=4800)
        let main1 = DetectedTextBox::new(Roi::new(100, 50, 40, 120), "日本語", 0.95);
        // Main kanji column 2: "学習" (X=40..80, Y=50..130, W=40, H=80, area=3200)
        let main2 = DetectedTextBox::new(Roi::new(40, 50, 40, 80), "学習", 0.95);
        // Ruby furigana on right flank of main1: "にほんご" (X=142..154, Y=55..145, W=12, H=90, area=1080)
        let ruby = DetectedTextBox::new(Roi::new(142, 55, 12, 90), "にほんご", 0.85);
        // Punctuation at bottom of main2: "。" (X=55..65, Y=135..145, W=10, H=10, area=100)
        let punct = DetectedTextBox::new(Roi::new(55, 135, 10, 10), "。", 0.90);

        let boxes = vec![main1.clone(), ruby, main2.clone(), punct.clone()];
        let filtered = filter_furigana(boxes, TextOrientation::Vertical);

        let texts: Vec<&str> = filtered.iter().map(|b| b.text.as_str()).collect();
        // Ruby "にほんご" must be pruned, punctuation "。" must be retained
        assert_eq!(texts, vec!["日本語", "学習", "。"]);
    }

    #[test]
    fn test_horizontal_furigana_pruning_retains_punctuation() {
        // Main horizontal line: "東京タワー" (X=100..260, Y=50..90, W=160, H=40, area=6400)
        let main = DetectedTextBox::new(Roi::new(100, 50, 160, 40), "東京タワー", 0.95);
        // Second line: "見物" (X=100..180, Y=110..150, W=80, H=40, area=3200)
        let main2 = DetectedTextBox::new(Roi::new(100, 110, 80, 40), "見物", 0.95);
        // Ruby furigana above main line: "とうきょう" (X=100..180, Y=32..44, W=80, H=12, area=960)
        let ruby = DetectedTextBox::new(Roi::new(100, 32, 80, 12), "とうきょう", 0.85);
        // Punctuation comma: "、" (X=185..195, Y=135..145, W=10, H=10, area=100)
        let punct = DetectedTextBox::new(Roi::new(185, 135, 10, 10), "、", 0.90);

        let boxes = vec![main.clone(), ruby, main2.clone(), punct.clone()];
        let filtered = filter_furigana(boxes, TextOrientation::Horizontal);

        let texts: Vec<&str> = filtered.iter().map(|b| b.text.as_str()).collect();
        // Ruby "とうきょう" must be pruned, comma "、" must be retained
        assert_eq!(texts, vec!["東京タワー", "見物", "、"]);
    }
}
