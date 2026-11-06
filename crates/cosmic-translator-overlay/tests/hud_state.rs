use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    KeyboardInteractivity, Layer,
};
use cosmic_translator_overlay::hud::SubtitleHudState;
use translator_core::ipc::{HudMessage, HudPosition, HudStatus};

/// State machine matching the exact message handling logic in
/// `crates/cosmic-translator-overlay/src/main.rs:177–224`.
#[derive(Debug, Default)]
struct HudOverlaySimulator {
    hud: SubtitleHudState,
    subtitle_seq: u64,
}

impl HudOverlaySimulator {
    fn new() -> Self {
        Self {
            hud: SubtitleHudState::default(),
            subtitle_seq: 0,
        }
    }

    fn handle_message(&mut self, msg: HudMessage) {
        match msg {
            HudMessage::ShowSubtitle { text, .. } => {
                self.subtitle_seq = self.subtitle_seq.wrapping_add(1);
                self.hud.current_text = text;
                self.hud.visible = true;
            }
            HudMessage::ClearSubtitle => {
                self.subtitle_seq = self.subtitle_seq.wrapping_add(1);
                self.hud.current_text.clear();
            }
            HudMessage::SetStatus { active, text } => {
                if active {
                    self.hud.status_badge = Some(text);
                } else {
                    self.hud.status_badge = None;
                }
            }
            HudMessage::SetHudStatus { status } => {
                self.hud.apply_status(status.as_ref());
            }
            HudMessage::UpdateStyle {
                font_size,
                opacity,
                duration_ms,
                position,
                offset_y,
            } => {
                self.hud.font_size = font_size;
                self.hud.opacity = opacity;
                self.hud.duration_ms = duration_ms;
                self.hud.position = position;
                self.hud.offset_y = offset_y;
            }
            HudMessage::SetVisible { visible } => {
                self.hud.visible = visible;
            }
            HudMessage::SetPosition { position, offset_y } => {
                self.hud.position = position;
                self.hud.offset_y = offset_y;
            }
            HudMessage::SetEditMode { edit_mode } => {
                self.hud.edit_mode = edit_mode;
            }
            HudMessage::Ping | HudMessage::Quit => {}
        }
    }
}

#[test]
fn test_status_badge_interleaving_never_corrupts_subtitle_text() {
    let mut sim = HudOverlaySimulator::new();

    // 1. Initial State
    assert_eq!(sim.hud.current_text, "");
    assert_eq!(sim.hud.status_badge, None);

    // 2. Subtitle arrives
    sim.handle_message(HudMessage::ShowSubtitle {
        text: "The hero unsheathed the ancient blade.".to_string(),
        original: Some("勇者は古の剣を抜いた。".to_string()),
        duration_ms: 0,
    });
    assert_eq!(
        sim.hud.current_text,
        "The hero unsheathed the ancient blade."
    );
    assert_eq!(sim.hud.status_badge, None);
    assert_eq!(sim.subtitle_seq, 1);

    // 3. SetStatus { active: true } arrives (e.g. rate limit pause begins)
    sim.handle_message(HudMessage::SetStatus {
        active: true,
        text: "⏳ Rate Limited (HTTP 429): 30s remaining".to_string(),
    });
    assert_eq!(
        sim.hud.current_text, "The hero unsheathed the ancient blade.",
        "SetStatus {{ active: true }} MUST NOT mutate current_text"
    );
    assert_eq!(
        sim.hud.status_badge.as_deref(),
        Some("⏳ Rate Limited (HTTP 429): 30s remaining")
    );
    assert_eq!(
        sim.subtitle_seq, 1,
        "SetStatus MUST NOT mutate subtitle_seq"
    );

    // 4. Repeated countdown ticks (30s -> 29s -> 28s)
    for remaining in (1..30).rev() {
        sim.handle_message(HudMessage::SetStatus {
            active: true,
            text: format!("⏳ Rate Limited (HTTP 429): {remaining}s remaining"),
        });
        assert_eq!(
            sim.hud.current_text, "The hero unsheathed the ancient blade.",
            "Countdown tick {remaining}s must not corrupt current_text"
        );
        assert_eq!(
            sim.hud.status_badge.as_deref(),
            Some(format!("⏳ Rate Limited (HTTP 429): {remaining}s remaining").as_str())
        );
        assert_eq!(sim.subtitle_seq, 1);
    }

    // 5. While status badge is STILL active, a new subtitle arrives!
    sim.handle_message(HudMessage::ShowSubtitle {
        text: "A strange creature appeared from the shadows!".to_string(),
        original: Some("影から奇妙な怪物が現れた！".to_string()),
        duration_ms: 0,
    });
    assert_eq!(
        sim.hud.current_text, "A strange creature appeared from the shadows!",
        "New subtitle must replace old subtitle text"
    );
    assert_eq!(
        sim.hud.status_badge.as_deref(),
        Some("⏳ Rate Limited (HTTP 429): 1s remaining"),
        "Status badge must remain intact when new subtitle arrives"
    );
    assert_eq!(sim.subtitle_seq, 2);

    // 6. SetStatus { active: false } arrives (rate limit lifted)
    sim.handle_message(HudMessage::SetStatus {
        active: false,
        text: String::new(),
    });
    assert_eq!(
        sim.hud.current_text, "A strange creature appeared from the shadows!",
        "SetStatus {{ active: false }} MUST NOT clear or alter current_text"
    );
    assert_eq!(
        sim.hud.status_badge, None,
        "Status badge must be cleared when active: false"
    );
    assert_eq!(sim.subtitle_seq, 2);

    // 7. ClearSubtitle arrives
    sim.handle_message(HudMessage::ClearSubtitle);
    assert_eq!(sim.hud.current_text, "");
    assert_eq!(sim.hud.status_badge, None);
    assert_eq!(sim.subtitle_seq, 3);

    // 8. SetStatus { active: true } with empty subtitle
    sim.handle_message(HudMessage::SetStatus {
        active: true,
        text: "Backend Connecting...".to_string(),
    });
    assert_eq!(sim.hud.current_text, "");
    assert_eq!(
        sim.hud.status_badge.as_deref(),
        Some("Backend Connecting...")
    );

    // 9. ClearSubtitle arrives while status badge is active
    sim.handle_message(HudMessage::ClearSubtitle);
    assert_eq!(sim.hud.current_text, "");
    assert_eq!(
        sim.hud.status_badge.as_deref(),
        Some("Backend Connecting..."),
        "ClearSubtitle must not clear active status badge"
    );
    assert_eq!(sim.subtitle_seq, 4);

    // 10. Deactivate status badge
    sim.handle_message(HudMessage::SetStatus {
        active: false,
        text: String::new(),
    });
    assert_eq!(sim.hud.current_text, "");
    assert_eq!(sim.hud.status_badge, None);
}

#[test]
fn test_adversarial_randomized_interleaving_fuzzing_10k_steps() {
    let mut sim = HudOverlaySimulator::new();

    // Pseudo-random linear congruential generator for reproducible fuzzing
    let mut seed: u64 = 0xDEADBEEFCAFE;
    let mut rng = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        seed
    };

    let sample_texts = [
        "",
        "   ",
        "Short line",
        "Very long sentence with lots of characters explaining lore and gameplay mechanics.",
        "マルチバイト日本語テキスト【勇者】『魔法を唱えた！』",
        "Unicode emojis: 🚀🎮🔥🛡️⚔️✨💯",
        "Special symbols: <>{}[]/\\|@#$%^&*()_+~`",
        "Multiline:\nLine 1\nLine 2\nLine 3\nLine 4",
    ];

    let sample_statuses = [
        "",
        "   ",
        "Rate limit: 10s",
        "⏳ HTTP 429: 30s remaining",
        "Ollama Local Model Offline",
        "Reconnecting PipeWire...",
    ];

    for step in 0..10_000 {
        let op = rng() % 7;
        let prev_text = sim.hud.current_text.clone();
        let prev_badge = sim.hud.status_badge.clone();
        let prev_seq = sim.subtitle_seq;

        match op {
            0 => {
                // ShowSubtitle
                let idx = (rng() as usize) % sample_texts.len();
                let text = sample_texts[idx].to_string();
                sim.handle_message(HudMessage::ShowSubtitle {
                    text: text.clone(),
                    original: None,
                    duration_ms: 0,
                });
                assert_eq!(sim.hud.current_text, text);
                assert_eq!(
                    sim.hud.status_badge, prev_badge,
                    "ShowSubtitle must not mutate status badge at step {step}"
                );
                assert_eq!(sim.subtitle_seq, prev_seq.wrapping_add(1));
            }
            1 => {
                // SetStatus active=true
                let idx = (rng() as usize) % sample_statuses.len();
                let status_text = sample_statuses[idx].to_string();
                sim.handle_message(HudMessage::SetStatus {
                    active: true,
                    text: status_text.clone(),
                });
                assert_eq!(
                    sim.hud.current_text, prev_text,
                    "SetStatus active=true must NOT modify current_text at step {step}"
                );
                assert_eq!(sim.hud.status_badge, Some(status_text));
                assert_eq!(
                    sim.subtitle_seq, prev_seq,
                    "SetStatus must not mutate subtitle_seq at step {step}"
                );
            }
            2 => {
                // SetStatus active=false
                sim.handle_message(HudMessage::SetStatus {
                    active: false,
                    text: String::new(),
                });
                assert_eq!(
                    sim.hud.current_text, prev_text,
                    "SetStatus active=false must NOT modify current_text at step {step}"
                );
                assert_eq!(sim.hud.status_badge, None);
                assert_eq!(sim.subtitle_seq, prev_seq);
            }
            3 => {
                // ClearSubtitle
                sim.handle_message(HudMessage::ClearSubtitle);
                assert_eq!(sim.hud.current_text, "");
                assert_eq!(
                    sim.hud.status_badge, prev_badge,
                    "ClearSubtitle must NOT modify status_badge at step {step}"
                );
                assert_eq!(sim.subtitle_seq, prev_seq.wrapping_add(1));
            }
            4 => {
                // UpdateStyle
                let pos = match rng() % 3 {
                    0 => HudPosition::Top,
                    1 => HudPosition::Center,
                    _ => HudPosition::Bottom,
                };
                sim.handle_message(HudMessage::UpdateStyle {
                    font_size: ((rng() % 40) + 10) as u16,
                    opacity: 0.85,
                    duration_ms: 0,
                    position: pos,
                    offset_y: (rng() % 100) as u32,
                });
                assert_eq!(sim.hud.current_text, prev_text);
                assert_eq!(sim.hud.status_badge, prev_badge);
            }
            5 => {
                // SetVisible
                let visible = (rng() % 2) == 1;
                sim.handle_message(HudMessage::SetVisible { visible });
                assert_eq!(sim.hud.current_text, prev_text);
                assert_eq!(sim.hud.status_badge, prev_badge);
            }
            _ => {
                // SetPosition
                let pos = match rng() % 3 {
                    0 => HudPosition::Top,
                    1 => HudPosition::Center,
                    _ => HudPosition::Bottom,
                };
                sim.handle_message(HudMessage::SetPosition {
                    position: pos,
                    offset_y: (rng() % 80) as u32,
                });
                assert_eq!(sim.hud.current_text, prev_text);
                assert_eq!(sim.hud.status_badge, prev_badge);
            }
        }

        // Verify layer settings invariants on every single step
        let settings = sim.hud.layer_settings();
        assert_eq!(settings.layer, Layer::Overlay);
        assert_eq!(settings.keyboard_interactivity, KeyboardInteractivity::None);
        assert!(settings.input_zone.is_some());
        assert!(settings.input_zone.as_ref().unwrap().is_empty());
        assert_eq!(settings.exclusive_zone, -1);

        // Verify view rendering never panics on every single step
        let _ = sim.hud.view(|_| ());
    }
}

#[test]
fn test_view_coexistence_and_extreme_content_rendering_zero_panics() {
    let mut hud = SubtitleHudState::default();

    // 1. Neither text nor badge (should render empty container)
    hud.current_text.clear();
    hud.status_badge = None;
    let _ = hud.view(|_| ());

    // 2. Whitespace only for both
    hud.current_text = "   \n\t  ".to_string();
    hud.status_badge = Some("   ".to_string());
    let _ = hud.view(|_| ());

    // 3. Status badge only (non-empty), current_text empty
    hud.current_text.clear();
    hud.status_badge = Some("⏳ Rate Limited (HTTP 429): 15s remaining".to_string());
    let _ = hud.view(|_| ());

    // 4. Subtitle text only (non-empty), status_badge None
    hud.current_text = "Standard dialogue subtitle text".to_string();
    hud.status_badge = None;
    let _ = hud.view(|_| ());

    // 5. Both subtitle text and status badge active (coexistence pill + badge)
    hud.current_text = "Standard dialogue subtitle text".to_string();
    hud.status_badge = Some("⏳ Rate Limited (HTTP 429): 15s remaining".to_string());
    let _ = hud.view(|_| ());

    // 6. Massive content stress (100,000 chars subtitle, 10,000 chars badge)
    hud.current_text = "あ".repeat(100_000);
    hud.status_badge = Some("⚠️".repeat(10_000));
    let _ = hud.view(|_| ());

    // 7. Multiline dialogue stress (1,000 lines)
    hud.current_text = (0..1000)
        .map(|i| format!("Dialogue line #{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    hud.status_badge = Some("Multi\nLine\nStatus\nBadge".to_string());
    let _ = hud.view(|_| ());

    // 8. Control characters, null bytes, RTL, CJK, emojis
    hud.current_text =
        "Hello\0World\x01\x02\r\nمرحبا بالعالم \u{202E}gnitselortnoc\u{202C} 🀄🐉🌸".to_string();
    hud.status_badge = Some("Status\0Null\x1F🚨".to_string());
    let _ = hud.view(|_| ());

    // 9. Extreme font sizes and opacities
    for font_size in [1, 10, 22, 72, 255] {
        hud.font_size = font_size;
        for opacity in [-10.0, 0.0, 0.1, 0.5, 0.92, 1.0, 10.0] {
            hud.opacity = opacity;
            let _ = hud.view(|_| ());
        }
    }

    // 10. Visibility toggling
    hud.visible = false;
    let _ = hud.view(|_| ());
    hud.visible = true;
    let _ = hud.view(|_| ());

    // 11. Edit mode rendering
    hud.edit_mode = true;
    let _ = hud.view(|_| ());
    hud.current_text.clear();
    let _ = hud.view(|_| ());
}

#[test]
fn typed_status_sets_and_clears_badge_without_touching_subtitle() {
    let mut sim = HudOverlaySimulator::new();
    sim.handle_message(HudMessage::ShowSubtitle {
        text: "Hello".to_string(),
        original: None,
        duration_ms: 0,
    });
    let seq = sim.subtitle_seq;

    sim.handle_message(HudMessage::SetHudStatus {
        status: Some(HudStatus::RateLimited {
            retry_after_secs: 30,
        }),
    });
    let badge = sim.hud.status_badge.clone().unwrap_or_default();
    assert!(
        badge.contains("30"),
        "rate-limit badge missing delay: {badge:?}"
    );
    assert_eq!(sim.hud.current_text, "Hello");
    assert_eq!(sim.subtitle_seq, seq);

    sim.handle_message(HudMessage::SetHudStatus {
        status: Some(HudStatus::OcrUnavailable {
            reason: "missing jpn".to_string(),
        }),
    });
    let badge = sim.hud.status_badge.clone().unwrap_or_default();
    assert!(
        badge.contains("missing jpn"),
        "OCR badge missing: {badge:?}"
    );

    sim.handle_message(HudMessage::SetHudStatus { status: None });
    assert!(sim.hud.status_badge.is_none());
    assert_eq!(sim.hud.current_text, "Hello");
}

#[test]
fn test_hud_status_badge_coexistence_with_subtitles() {
    let mut hud = SubtitleHudState {
        current_text: "Active dialogue text".to_string(),
        ..Default::default()
    };
    assert_eq!(hud.current_text, "Active dialogue text");
    assert!(hud.status_badge.is_none());

    // 1. Verify layer settings input zone is strictly empty vec for click-through
    let settings = hud.layer_settings();
    assert!(settings.input_zone.is_some());
    assert!(settings.input_zone.as_ref().unwrap().is_empty());

    // 2. Set status badge active
    hud.status_badge = Some("⏳ Rate Limited (HTTP 429): 20s remaining".to_string());
    assert_eq!(
        hud.current_text, "Active dialogue text",
        "current_text must NOT be overwritten"
    );
    assert_eq!(
        hud.status_badge.as_deref(),
        Some("⏳ Rate Limited (HTTP 429): 20s remaining")
    );

    // 3. Render view with both badge and text
    let _ = hud.view(|_| ());

    // 4. Deactivate status badge
    hud.status_badge = None;
    assert_eq!(
        hud.current_text, "Active dialogue text",
        "current_text must remain intact when badge cleared"
    );
    assert!(hud.status_badge.is_none());

    // 5. Render view with text only
    let _ = hud.view(|_| ());

    // 6. Clear text and render with badge only
    hud.current_text.clear();
    hud.status_badge = Some("Status only".to_string());
    let _ = hud.view(|_| ());
}
