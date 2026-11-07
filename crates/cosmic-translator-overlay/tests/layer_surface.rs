use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer,
};
use cosmic_translator_overlay::hud::SubtitleHudState;
use translator_protocol::HudPosition;

#[test]
fn test_clickthrough_input_zone_is_strictly_empty_vec_in_normal_mode() {
    let mut hud = SubtitleHudState::default();

    let test_statuses = [
        None,
        Some("".to_string()),
        Some("   ".to_string()),
        Some("⏳ Rate Limited (HTTP 429): 30s remaining".to_string()),
        Some("Long status message spanning multiple words with emojis 🎮🔥".to_string()),
        Some("A".repeat(5000)),
    ];

    let test_texts = [
        "".to_string(),
        "Active subtitle".to_string(),
        "B".repeat(10000),
    ];

    let test_positions = [HudPosition::Top, HudPosition::Center, HudPosition::Bottom];
    let test_offsets = [0, 20, 40, 100, 1000];

    for status in &test_statuses {
        hud.status_badge = status.clone();

        for text in &test_texts {
            hud.current_text = text.clone();

            for pos in test_positions {
                hud.position = pos;

                for offset in test_offsets {
                    hud.offset_y = offset;

                    // NORMAL MODE (edit_mode = false)
                    hud.edit_mode = false;
                    let settings = hud.layer_settings();

                    // Invariant 1: input_zone MUST be Some(vec![])
                    assert!(
                        settings.input_zone.is_some(),
                        "Normal mode input_zone must be Some"
                    );
                    assert!(
                        settings.input_zone.as_ref().unwrap().is_empty(),
                        "Normal mode input_zone must be empty vec for click-through"
                    );

                    // Invariant 2: keyboard_interactivity MUST be None
                    assert_eq!(
                        settings.keyboard_interactivity,
                        KeyboardInteractivity::None,
                        "Normal mode must NEVER request keyboard focus"
                    );

                    // Invariant 3: layer MUST be Overlay
                    assert_eq!(
                        settings.layer,
                        Layer::Overlay,
                        "Layer must always be Overlay"
                    );

                    // Invariant 4: exclusive_zone MUST be -1
                    assert_eq!(
                        settings.exclusive_zone, -1,
                        "Exclusive zone must be -1 to prevent window shifting"
                    );

                    // Invariant 5: Anchor correct
                    match pos {
                        HudPosition::Top => {
                            assert!(settings.anchor.contains(Anchor::TOP));
                            assert!(settings.anchor.contains(Anchor::LEFT | Anchor::RIGHT));
                        }
                        HudPosition::Bottom => {
                            assert!(settings.anchor.contains(Anchor::BOTTOM));
                            assert!(settings.anchor.contains(Anchor::LEFT | Anchor::RIGHT));
                        }
                        HudPosition::Center => {
                            assert!(settings.anchor.contains(Anchor::LEFT | Anchor::RIGHT));
                            assert!(!settings.anchor.contains(Anchor::TOP));
                            assert!(!settings.anchor.contains(Anchor::BOTTOM));
                        }
                    }

                    // EDIT MODE (edit_mode = true)
                    hud.edit_mode = true;
                    let edit_settings = hud.layer_settings();

                    // In edit mode: input_zone is None (accepts clicks)
                    assert!(
                        edit_settings.input_zone.is_none(),
                        "Edit mode input_zone must be None so calibration controls can be clicked"
                    );
                    assert_eq!(
                        edit_settings.keyboard_interactivity,
                        KeyboardInteractivity::OnDemand,
                        "Edit mode must allow keyboard interactivity"
                    );

                    // Toggling back to false immediately restores click-through
                    hud.edit_mode = false;
                    let restored_settings = hud.layer_settings();
                    assert!(
                        restored_settings.input_zone.as_ref().unwrap().is_empty(),
                        "Restoring edit_mode=false must immediately restore input_zone click-through"
                    );
                }
            }
        }
    }
}

#[test]
fn test_hud_layer_shell_clickthrough_contract() {
    let hud = SubtitleHudState::default();
    let settings = hud.layer_settings();

    // 1. Must be rendered on Overlay layer
    assert_eq!(settings.layer, Layer::Overlay);

    // 2. Must NEVER request keyboard focus (prevents focus stealing from games)
    assert_eq!(settings.keyboard_interactivity, KeyboardInteractivity::None);

    // 3. CRITICAL: input_zone must be Some(vec![])
    // In zwlr_layer_shell_v1, an empty region explicitly tells the compositor
    // to pass all pointer/touch/cursor events through to windows underneath.
    assert!(
        settings.input_zone.is_some(),
        "input_zone must be explicitly set"
    );
    assert!(
        settings.input_zone.as_ref().unwrap().is_empty(),
        "input_zone must be empty vec for 100% click-through guarantee"
    );

    // 4. Must not allocate exclusive margins to avoid shifting game viewports
    assert_eq!(settings.exclusive_zone, -1);

    // 5. Must anchor to bottom of screen
    assert!(settings.anchor.contains(Anchor::BOTTOM));
}
