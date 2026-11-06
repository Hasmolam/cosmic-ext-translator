use image::{imageops::FilterType, RgbaImage};

/// Computes a 64-bit difference hash (dHash) for an image.
///
/// dHash resizes the image to 9x8, converts to luminance, and compares
/// adjacent horizontal pixels. Each comparison yields 1 bit, producing a
/// robust 64-bit perceptual fingerprint resistant to minor scaling and compression artifacts.
#[must_use]
pub fn dhash_64(image: &RgbaImage) -> u64 {
    let small = image::imageops::resize(image, 9, 8, FilterType::Triangle);
    let mut hash = 0u64;

    for y in 0..8 {
        for x in 0..8 {
            let left_pixel = small.get_pixel(x, y);
            let right_pixel = small.get_pixel(x + 1, y);

            let left_lum = rgb_to_luminance(left_pixel[0], left_pixel[1], left_pixel[2]);
            let right_lum = rgb_to_luminance(right_pixel[0], right_pixel[1], right_pixel[2]);

            if left_lum > right_lum {
                let bit_index = y * 8 + x;
                hash |= 1u64 << bit_index;
            }
        }
    }

    hash
}

/// Computes the Hamming distance (number of differing bits) between two 64-bit hashes.
#[inline]
#[must_use]
pub const fn hamming_distance_64(h1: u64, h2: u64) -> u32 {
    (h1 ^ h2).count_ones()
}

/// Computes a higher resolution 256-bit difference hash (17x16 sample grid).
#[must_use]
pub fn dhash_256(image: &RgbaImage) -> [u64; 4] {
    let small = image::imageops::resize(image, 17, 16, FilterType::Triangle);
    let mut hashes = [0u64; 4];

    for y in 0..16 {
        for x in 0..16 {
            let left_pixel = small.get_pixel(x, y);
            let right_pixel = small.get_pixel(x + 1, y);

            let left_lum = rgb_to_luminance(left_pixel[0], left_pixel[1], left_pixel[2]);
            let right_lum = rgb_to_luminance(right_pixel[0], right_pixel[1], right_pixel[2]);

            if left_lum > right_lum {
                let bit_pos = y * 16 + x;
                let chunk_idx = (bit_pos / 64) as usize;
                let bit_offset = bit_pos % 64;
                hashes[chunk_idx] |= 1u64 << bit_offset;
            }
        }
    }

    hashes
}

/// Computes the Hamming distance between two 256-bit hashes.
#[inline]
#[must_use]
pub const fn hamming_distance_256(h1: &[u64; 4], h2: &[u64; 4]) -> u32 {
    (h1[0] ^ h2[0]).count_ones()
        + (h1[1] ^ h2[1]).count_ones()
        + (h1[2] ^ h2[2]).count_ones()
        + (h1[3] ^ h2[3]).count_ones()
}

#[inline]
const fn rgb_to_luminance(r: u8, g: u8, b: u8) -> u16 {
    // Fast integer approximation of ITU-R BT.601: Y = (77*R + 150*G + 29*B) >> 8
    (77 * r as u16 + 150 * g as u16 + 29 * b as u16) >> 8
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_identical_images_have_zero_distance() {
        let img = RgbaImage::from_pixel(100, 100, Rgba([120, 150, 200, 255]));
        let h1 = dhash_64(&img);
        let h2 = dhash_64(&img);
        assert_eq!(hamming_distance_64(h1, h2), 0);

        let h256_1 = dhash_256(&img);
        let h256_2 = dhash_256(&img);
        assert_eq!(hamming_distance_256(&h256_1, &h256_2), 0);
    }

    #[test]
    fn test_gradient_and_different_images() {
        let mut img1 = RgbaImage::new(100, 100);
        let mut img2 = RgbaImage::new(100, 100);

        // Pattern 1: Left half bright, right half dark
        for y in 0..100 {
            for x in 0..100 {
                let v = if x < 50 { 255 } else { 0 };
                img1.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }

        // Pattern 2: Left half dark, right half bright
        for y in 0..100 {
            for x in 0..100 {
                let v = if x < 50 { 0 } else { 255 };
                img2.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }

        let h1 = dhash_64(&img1);
        let h2 = dhash_64(&img2);
        let dist = hamming_distance_64(h1, h2);
        assert!(
            dist > 5,
            "Inverted patterns should have high distance: {dist}"
        );
    }
}
