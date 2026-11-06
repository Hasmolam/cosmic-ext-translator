use translator_core::region::Roi;

#[test]
fn test_overlay_stdout_communication_protocol() {
    #[derive(serde::Deserialize)]
    struct SelectedPayload {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        surface_width: Option<u32>,
        surface_height: Option<u32>,
    }

    // 1. Simulate exact output emitted by cosmic-translator-overlay writeln!
    let simulated_roi = Roi::new(150, 250, 800, 450);
    let surface_w = 1920;
    let surface_h = 1080;
    let mut stdout_buf = Vec::new();
    use std::io::Write;
    let _ = writeln!(
        &mut stdout_buf,
        "ROI_SELECTED:{{\"x\":{},\"y\":{},\"width\":{},\"height\":{},\"surface_width\":{},\"surface_height\":{}}}",
        simulated_roi.x, simulated_roi.y, simulated_roi.width, simulated_roi.height, surface_w, surface_h
    );

    let stdout_str = String::from_utf8_lossy(&stdout_buf);
    let mut parsed_roi = None;
    let mut parsed_sw = None;
    let mut parsed_sh = None;
    for line in stdout_str.lines() {
        if let Some(json_str) = line.strip_prefix("ROI_SELECTED:") {
            if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str) {
                parsed_roi = Some(Roi::new(p.x, p.y, p.width, p.height));
                parsed_sw = p.surface_width;
                parsed_sh = p.surface_height;
            }
        }
    }

    assert_eq!(parsed_roi, Some(simulated_roi));
    assert_eq!(parsed_sw, Some(surface_w));
    assert_eq!(parsed_sh, Some(surface_h));

    // 2. Cancellation output
    let mut cancel_buf = Vec::new();
    let _ = writeln!(&mut cancel_buf, "ROI_CANCELLED");
    let cancel_str = String::from_utf8_lossy(&cancel_buf);
    let is_cancelled = cancel_str.lines().any(|l| l.trim() == "ROI_CANCELLED");
    assert!(is_cancelled);

    // 3. Multi-line noise with trailing output
    let noisy_output = "INFO: starting layer surface\nDEBUG: attached to output\nROI_SELECTED:{\"x\":10,\"y\":20,\"width\":30,\"height\":40,\"surface_width\":1000,\"surface_height\":800}\nINFO: layer surface destroyed\n";
    let mut found = None;
    for line in noisy_output.lines() {
        if let Some(json_str) = line.strip_prefix("ROI_SELECTED:") {
            if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str) {
                found = Some(Roi::new(p.x, p.y, p.width, p.height));
            }
        }
    }
    assert_eq!(found, Some(Roi::new(10, 20, 30, 40)));

    // 4. Missing optional surface dimensions
    let no_surface_json = "ROI_SELECTED:{\"x\":5,\"y\":15,\"width\":25,\"height\":35}";
    let mut no_surface_roi = None;
    let mut no_surface_w = None;
    if let Some(json_str) = no_surface_json.strip_prefix("ROI_SELECTED:") {
        if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str) {
            no_surface_roi = Some(Roi::new(p.x, p.y, p.width, p.height));
            no_surface_w = p.surface_width;
        }
    }
    assert_eq!(no_surface_roi, Some(Roi::new(5, 15, 25, 35)));
    assert_eq!(no_surface_w, None);

    // 5. Malformed json payload handled gracefully without panic
    let malformed_cases = [
        "ROI_SELECTED:",
        "ROI_SELECTED:{}",
        "ROI_SELECTED:{\"x\":\"not_a_number\"}",
        "ROI_SELECTED:invalid_json_data",
        "ROI_SELECTED:{\"x\":-1,\"y\":0,\"width\":10,\"height\":10}",
    ];
    for malformed in malformed_cases {
        let mut parsed = false;
        if let Some(json_str) = malformed.strip_prefix("ROI_SELECTED:") {
            if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str) {
                let _ = Roi::new(p.x, p.y, p.width, p.height);
                parsed = true;
            }
        }
        assert!(
            !parsed,
            "Malformed case '{malformed}' should not parse successfully"
        );
    }
}
