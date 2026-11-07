//! Runs in its own process because it points `XDG_CONFIG_HOME` at a temporary directory.

use cosmic_translator_overlay::hud::save_position;
use translator_protocol::{HudPosition, APPLET_CONFIG_ID, APPLET_CONFIG_VERSION};

#[test]
fn calibrated_position_is_written_to_applet_config() {
    let base = std::env::temp_dir().join(format!("cet-hud-config-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    std::env::set_var("XDG_CONFIG_HOME", &base);

    save_position(HudPosition::Top, 120).unwrap();

    let dir = base
        .join("cosmic")
        .join(APPLET_CONFIG_ID)
        .join(format!("v{APPLET_CONFIG_VERSION}"));
    let position = std::fs::read_to_string(dir.join("hud_position")).unwrap();
    let offset = std::fs::read_to_string(dir.join("hud_offset_y")).unwrap();
    assert_eq!(position.trim(), "\"top\"");
    assert_eq!(offset.trim(), "120");

    let _ = std::fs::remove_dir_all(&base);
}
