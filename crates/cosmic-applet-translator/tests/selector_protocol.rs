use translator_core::ipc::selector::{SelectorOutput, CANCELLED_LINE};
use translator_core::region::Roi;

#[test]
fn test_overlay_stdout_communication_protocol() {
    // 1. Line produced by cosmic-translator-overlay on selection
    let selected = SelectorOutput::new(Roi::new(150, 250, 800, 450), 1920, 1080);
    let line = selected.to_line().unwrap();
    assert_eq!(
        line,
        "ROI_SELECTED:{\"x\":150,\"y\":250,\"width\":800,\"height\":450,\"surface_width\":1920,\"surface_height\":1080}"
    );
    let parsed = SelectorOutput::parse_stdout(&format!("{line}\n")).unwrap();
    assert_eq!(parsed.roi(), Roi::new(150, 250, 800, 450));
    assert_eq!(parsed.surface_width, Some(1920));
    assert_eq!(parsed.surface_height, Some(1080));

    // 2. Cancellation output yields no selection
    assert_eq!(
        SelectorOutput::parse_stdout(&format!("{CANCELLED_LINE}\n")),
        None
    );

    // 3. Multi-line noise with trailing output
    let noisy_output = "INFO: starting layer surface\nDEBUG: attached to output\nROI_SELECTED:{\"x\":10,\"y\":20,\"width\":30,\"height\":40,\"surface_width\":1000,\"surface_height\":800}\nINFO: layer surface destroyed\n";
    assert_eq!(
        SelectorOutput::parse_stdout(noisy_output).map(|o| o.roi()),
        Some(Roi::new(10, 20, 30, 40))
    );

    // 4. Missing optional surface dimensions
    let no_surface =
        SelectorOutput::parse_line("ROI_SELECTED:{\"x\":5,\"y\":15,\"width\":25,\"height\":35}")
            .unwrap();
    assert_eq!(no_surface.roi(), Roi::new(5, 15, 25, 35));
    assert_eq!(no_surface.surface_width, None);

    // 5. Malformed json payload handled gracefully without panic
    let malformed_cases = [
        "ROI_SELECTED:",
        "ROI_SELECTED:{}",
        "ROI_SELECTED:{\"x\":\"not_a_number\"}",
        "ROI_SELECTED:invalid_json_data",
        "ROI_SELECTED:{\"x\":-1,\"y\":0,\"width\":10,\"height\":10}",
    ];
    for malformed in malformed_cases {
        assert_eq!(
            SelectorOutput::parse_line(malformed),
            None,
            "Malformed case '{malformed}' should not parse successfully"
        );
    }
}
