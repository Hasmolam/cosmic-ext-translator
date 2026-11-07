//! Axis-aligned rectangle in pixel coordinates.

/// Bounding box of a recognised token, in pixels with the origin at the top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Rect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Rect {
    /// Creates a rectangle from its top-left corner and size.
    #[must_use]
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// One past the rightmost column, saturating at `u32::MAX`.
    #[must_use]
    pub const fn right(&self) -> u32 {
        self.x.saturating_add(self.width)
    }

    /// One past the bottom row, saturating at `u32::MAX`.
    #[must_use]
    pub const fn bottom(&self) -> u32 {
        self.y.saturating_add(self.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_saturate() {
        let r = Rect::new(u32::MAX - 1, 10, 5, 20);
        assert_eq!(r.right(), u32::MAX);
        assert_eq!(r.bottom(), 30);
    }
}
