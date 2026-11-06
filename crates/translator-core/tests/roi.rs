use image::{Rgba, RgbaImage};
use translator_core::capture::roi::{CaptureRegion, Roi, RoiPreset};

#[test]
fn test_adversarial_mask_region_extreme_bounds_and_overflow() {
    let white = Rgba([255, 255, 255, 255]);
    let black = Rgba([0, 0, 0, 255]);

    // Test 1: Coordinates near u32::MAX
    {
        let target = Roi::new(u32::MAX - 100, u32::MAX - 100, 50, 50);
        let ignore = Roi::new(u32::MAX - 80, u32::MAX - 80, 50, 50);
        let mut img = RgbaImage::from_pixel(50, 50, white);

        // Must not panic or cause arithmetic overflow
        target.mask_region(&mut img, &ignore);

        // Expected overlap in full frame:
        // inter_left = max(MAX-100, MAX-80) = MAX-80
        // inter_right = min(MAX-50, MAX-30) = MAX-50
        // local_x = (MAX-80) - (MAX-100) = 20 .. (MAX-50) - (MAX-100) = 50
        for y in 0..50 {
            for x in 0..50 {
                let p = *img.get_pixel(x, y);
                if (20..50).contains(&x) && (20..50).contains(&y) {
                    assert_eq!(p, black, "Pixel ({x}, {y}) should be masked black");
                } else {
                    assert_eq!(p, white, "Pixel ({x}, {y}) should be white");
                }
            }
        }
    }

    // Test 2: Extreme values near u32::MAX with addition overflow potential
    {
        // Target is at u32::MAX - 5, width 100.
        // saturating_add clamps target_right to u32::MAX.
        // Ignore is at u32::MAX - 10, width 200 (clamped to u32::MAX).
        // Overlap in u32 space is [u32::MAX - 5, u32::MAX] = 5 pixels wide.
        let target = Roi::new(u32::MAX - 5, u32::MAX - 5, 100, 100);
        let ignore = Roi::new(u32::MAX - 10, u32::MAX - 10, 200, 200);
        let mut img = RgbaImage::from_pixel(20, 20, white);

        // Must execute cleanly without overflow panics
        target.mask_region(&mut img, &ignore);

        // First 5x5 pixels are in bounds of the intersection; remaining are white
        for y in 0..20 {
            for x in 0..20 {
                let p = *img.get_pixel(x, y);
                if x < 5 && y < 5 {
                    assert_eq!(p, black);
                } else {
                    assert_eq!(p, white);
                }
            }
        }

        // Now test where target fully fits within u32::MAX
        let target_fit = Roi::new(u32::MAX - 20, u32::MAX - 20, 20, 20);
        let ignore_fit = Roi::new(u32::MAX - 25, u32::MAX - 25, 30, 30);
        let mut img_fit = RgbaImage::from_pixel(20, 20, white);
        target_fit.mask_region(&mut img_fit, &ignore_fit);
        assert!(
            img_fit.pixels().all(|p| *p == black),
            "Fully enclosed target near u32::MAX should be 100% masked"
        );
    }

    // Test 3: Zero-sized rectangles (width = 0 or height = 0 or both)
    {
        let target = Roi::new(10, 10, 50, 50);
        let zero_cases = [
            Roi::new(10, 10, 0, 50),
            Roi::new(10, 10, 50, 0),
            Roi::new(10, 10, 0, 0),
            Roi::new(0, 0, 0, 0),
            Roi::new(u32::MAX, u32::MAX, 0, 0),
        ];

        for (idx, z_roi) in zero_cases.iter().enumerate() {
            let mut img = RgbaImage::from_pixel(50, 50, white);
            target.mask_region(&mut img, z_roi);
            assert!(
                img.pixels().all(|p| *p == white),
                "Zero-sized ignore case {idx} should perform zero writes"
            );
        }

        // Target itself zero-sized
        let mut img = RgbaImage::from_pixel(50, 50, white);
        Roi::new(10, 10, 0, 50).mask_region(&mut img, &Roi::new(10, 10, 50, 50));
        assert!(img.pixels().all(|p| *p == white));
    }

    // Test 4: Thin 1-pixel strips
    {
        let target = Roi::new(100, 100, 50, 50);

        // 1-pixel vertical strip at x = 110
        let ignore_vert = Roi::new(110, 100, 1, 50);
        let mut img = RgbaImage::from_pixel(50, 50, white);
        target.mask_region(&mut img, &ignore_vert);

        let mut masked_count = 0;
        for y in 0..50 {
            for x in 0..50 {
                if *img.get_pixel(x, y) == black {
                    assert_eq!(x, 10, "Only column x=10 should be black");
                    masked_count += 1;
                }
            }
        }
        assert_eq!(
            masked_count, 50,
            "Expected exactly 50 masked pixels for 1x50 strip"
        );

        // 1-pixel horizontal strip at y = 125
        let ignore_horiz = Roi::new(100, 125, 50, 1);
        let mut img2 = RgbaImage::from_pixel(50, 50, white);
        target.mask_region(&mut img2, &ignore_horiz);

        let mut masked_horiz_count = 0;
        for y in 0..50 {
            for x in 0..50 {
                if *img2.get_pixel(x, y) == black {
                    assert_eq!(y, 25, "Only row y=25 should be black");
                    masked_horiz_count += 1;
                }
            }
        }
        assert_eq!(
            masked_horiz_count, 50,
            "Expected exactly 50 masked pixels for 50x1 strip"
        );

        // 1-pixel single pixel at (115, 115)
        let ignore_single = Roi::new(115, 115, 1, 1);
        let mut img3 = RgbaImage::from_pixel(50, 50, white);
        target.mask_region(&mut img3, &ignore_single);

        let black_count = img3.pixels().filter(|p| **p == black).count();
        assert_eq!(black_count, 1, "Expected exactly 1 masked pixel");
        assert_eq!(*img3.get_pixel(15, 15), black);
    }

    // Test 5: Overlapping all four corners
    {
        let target = Roi::new(100, 100, 40, 40);

        // Top-Left corner overlap
        let mut img_tl = RgbaImage::from_pixel(40, 40, white);
        target.mask_region(&mut img_tl, &Roi::new(90, 90, 20, 20));
        // Overlap: [100..110) x [100..110) -> local [0..10) x [0..10)
        assert_eq!(img_tl.pixels().filter(|p| **p == black).count(), 100);
        assert_eq!(*img_tl.get_pixel(0, 0), black);
        assert_eq!(*img_tl.get_pixel(9, 9), black);
        assert_eq!(*img_tl.get_pixel(10, 10), white);

        // Top-Right corner overlap
        let mut img_tr = RgbaImage::from_pixel(40, 40, white);
        target.mask_region(&mut img_tr, &Roi::new(130, 90, 20, 20));
        // Overlap: [130..140) x [100..110) -> local [30..40) x [0..10)
        assert_eq!(img_tr.pixels().filter(|p| **p == black).count(), 100);
        assert_eq!(*img_tr.get_pixel(30, 0), black);
        assert_eq!(*img_tr.get_pixel(39, 9), black);
        assert_eq!(*img_tr.get_pixel(29, 0), white);

        // Bottom-Left corner overlap
        let mut img_bl = RgbaImage::from_pixel(40, 40, white);
        target.mask_region(&mut img_bl, &Roi::new(90, 130, 20, 20));
        // Overlap: [100..110) x [130..140) -> local [0..10) x [30..40)
        assert_eq!(img_bl.pixels().filter(|p| **p == black).count(), 100);
        assert_eq!(*img_bl.get_pixel(0, 30), black);
        assert_eq!(*img_bl.get_pixel(9, 39), black);
        assert_eq!(*img_bl.get_pixel(0, 29), white);

        // Bottom-Right corner overlap
        let mut img_br = RgbaImage::from_pixel(40, 40, white);
        target.mask_region(&mut img_br, &Roi::new(130, 130, 20, 20));
        // Overlap: [130..140) x [130..140) -> local [30..40) x [30..40)
        assert_eq!(img_br.pixels().filter(|p| **p == black).count(), 100);
        assert_eq!(*img_br.get_pixel(30, 30), black);
        assert_eq!(*img_br.get_pixel(39, 39), black);
        assert_eq!(*img_br.get_pixel(29, 29), white);
    }

    // Test 6: Ignore region far larger than the canvas
    {
        let target = Roi::new(500, 500, 100, 100);
        let gigantic_ignore = Roi::new(0, 0, u32::MAX, u32::MAX);
        let mut img = RgbaImage::from_pixel(100, 100, white);

        target.mask_region(&mut img, &gigantic_ignore);
        assert!(
            img.pixels().all(|p| *p == black),
            "Ignore larger than canvas should mask entire target ROI without panics"
        );
    }

    // Test 7: Buffer dimensions smaller than target ROI width/height
    {
        let target = Roi::new(0, 0, 1000, 1000);
        let ignore = Roi::new(0, 0, 500, 500);
        // Image buffer is only 50x50
        let mut img = RgbaImage::from_pixel(50, 50, white);

        target.mask_region(&mut img, &ignore);
        // Must clamp to img dimensions (50x50) and not panic out of bounds
        assert!(img.pixels().all(|p| *p == black));
    }
}

#[test]
fn test_adversarial_mask_region_random_fuzzing() {
    let white = Rgba([255, 255, 255, 255]);
    let black = Rgba([0, 0, 0, 255]);

    // Pseudo-random linear congruential generator for reproducible deterministic fuzzing
    let mut state: u64 = 0xDEADBEEFCAFEBABE;
    let mut next_u32 = || -> u32 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 32) as u32
    };

    // Extreme coordinate candidate values to mix with random values
    let extreme_coords = [
        0,
        1,
        2,
        50,
        100,
        u32::MAX / 2,
        u32::MAX - 1000,
        u32::MAX - 50,
        u32::MAX - 1,
        u32::MAX,
    ];

    for iteration in 0..5_000 {
        let pick_coord = |r: u32| -> u32 {
            if r.is_multiple_of(3) {
                extreme_coords[(r as usize / 3) % extreme_coords.len()]
            } else {
                r
            }
        };

        let tx = pick_coord(next_u32());
        let ty = pick_coord(next_u32());
        let tw = pick_coord(next_u32());
        let th = pick_coord(next_u32());

        let ix = pick_coord(next_u32());
        let iy = pick_coord(next_u32());
        let iw = pick_coord(next_u32());
        let ih = pick_coord(next_u32());

        let target = Roi::new(tx, ty, tw, th);
        let ignore = Roi::new(ix, iy, iw, ih);

        // Frame dimensions between 0 and 64
        let img_w = next_u32() % 65;
        let img_h = next_u32() % 65;

        let mut img = RgbaImage::from_pixel(img_w, img_h, white);

        // Verification requirement: ZERO panics, out-of-bounds writes, or arithmetic overflows
        target.mask_region(&mut img, &ignore);

        // Verify that only black or white pixels exist in the buffer (no garbage/memory corruption)
        for p in img.pixels() {
            assert!(
                *p == white || *p == black,
                "Fuzz iteration {iteration}: pixel corrupted to {:?}",
                p
            );
        }
    }
}

#[tokio::test]
async fn test_roi_cropping_formats_and_strides() {
    let width = 200;
    let height = 100;
    let effective_stride = (width * 4) as usize;

    // 1. Test RGBA format
    let mut rgba_raw = vec![0u8; height as usize * effective_stride];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * effective_stride + x * 4;
            rgba_raw[offset] = 200; // R
            rgba_raw[offset + 1] = 100; // G
            rgba_raw[offset + 2] = 50; // B
            rgba_raw[offset + 3] = 255; // A
        }
    }

    let roi = Roi::new(20, 20, 50, 40);
    let cropped = roi
        .crop_to_rgba(&rgba_raw, width, height, effective_stride, false)
        .expect("RGBA cropping should succeed");

    assert_eq!(cropped.width(), 50);
    assert_eq!(cropped.height(), 40);
    assert_eq!(cropped.get_pixel(0, 0), &Rgba([200, 100, 50, 255]));

    // 2. Test BGRx format (swapped B and R channels)
    let mut bgrx_raw = vec![0u8; height as usize * effective_stride];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * effective_stride + x * 4;
            bgrx_raw[offset] = 50; // B
            bgrx_raw[offset + 1] = 100; // G
            bgrx_raw[offset + 2] = 200; // R
            bgrx_raw[offset + 3] = 255; // x
        }
    }

    let cropped_bgr = roi
        .crop_to_rgba(&bgrx_raw, width, height, effective_stride, true)
        .expect("BGRx cropping should succeed");

    // After swapping, R should be 200 and B should be 50
    assert_eq!(cropped_bgr.get_pixel(0, 0), &Rgba([200, 100, 50, 255]));

    // 3. Test non-standard padded stride (e.g. GPU buffer row alignment)
    let padded_stride = effective_stride + 128; // 128 bytes padding per line
    let mut padded_raw = vec![0u8; height as usize * padded_stride];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let offset = y * padded_stride + x * 4;
            padded_raw[offset] = 123;
            padded_raw[offset + 1] = 45;
            padded_raw[offset + 2] = 67;
            padded_raw[offset + 3] = 255;
        }
    }

    let cropped_padded = roi
        .crop_to_rgba(&padded_raw, width, height, padded_stride, false)
        .expect("Padded stride cropping should succeed");
    assert_eq!(cropped_padded.get_pixel(10, 10), &Rgba([123, 45, 67, 255]));
}

#[tokio::test]
async fn test_capture_region_presets_computation() {
    let screen_w = 1920;
    let screen_h = 1080;

    // FullScreen preset
    let full = CaptureRegion::new(RoiPreset::FullScreen, None, None);
    let full_roi = full.compute(screen_w, screen_h).unwrap();
    assert_eq!(full_roi.width, 1920);
    assert_eq!(full_roi.height, 1080);
    assert_eq!(full_roi.x, 0);
    assert_eq!(full_roi.y, 0);

    // Bottom 25% preset (typical RPG dialogue box: 5% margin each side)
    let b25 = CaptureRegion::new(RoiPreset::Bottom25, None, None);
    let b25_roi = b25.compute(screen_w, screen_h).unwrap();
    assert_eq!(b25_roi.x, 96);
    assert_eq!(b25_roi.y, 810);
    assert_eq!(b25_roi.width, 1728);
    assert_eq!(b25_roi.height, 270);

    // Custom preset
    let custom_preset =
        CaptureRegion::new(RoiPreset::Custom, Some(Roi::new(50, 60, 400, 200)), None);
    let custom_roi = custom_preset.compute(screen_w, screen_h).unwrap();
    assert_eq!(custom_roi.x, 50);
    assert_eq!(custom_roi.y, 60);
    assert_eq!(custom_roi.width, 400);
    assert_eq!(custom_roi.height, 200);
}
