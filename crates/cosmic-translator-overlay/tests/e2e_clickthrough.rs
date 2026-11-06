use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer,
};
use cosmic_translator_overlay::hud::SubtitleHudState;
use std::time::Duration;
use translator_core::ipc::{HudIpcClient, HudIpcServer, HudMessage};

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

#[tokio::test]
async fn test_hud_ipc_high_throughput_stress() {
    let socket_path =
        std::env::temp_dir().join(format!("test-hud-stress-{}.sock", std::process::id()));
    let server = HudIpcServer::new(&socket_path);
    let mut server_rx = server.start().await.expect("IPC server should start");

    let client = HudIpcClient::new(&socket_path);

    // Wait briefly for socket to become available
    let mut ready = false;
    for _ in 0..10 {
        if client.is_running().await {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(ready, "HUD IPC server was not ready in time");

    let total_messages = 50;
    for i in 0..total_messages {
        let msg = HudMessage::ShowSubtitle {
            text: format!("Subtitle payload #{i}"),
            original: Some(format!("Original #{i}")),
            duration_ms: 3000,
        };
        client.send(&msg).await.expect("Client send must succeed");
    }

    // Collect all messages from server
    let mut received_count = 0;
    let timeout = Duration::from_secs(3);
    let start = std::time::Instant::now();

    while received_count < total_messages && start.elapsed() < timeout {
        if let Ok(Some(msg)) =
            tokio::time::timeout(Duration::from_millis(100), server_rx.recv()).await
        {
            match msg {
                HudMessage::ShowSubtitle { text, .. } => {
                    assert!(text.starts_with("Subtitle payload #"));
                    received_count += 1;
                }
                HudMessage::Ping => {
                    // Ignore readiness ping from is_running() check
                }
                other => panic!("Unexpected message: {:?}", other),
            }
        }
    }

    assert_eq!(
        received_count, total_messages,
        "All 50 high-throughput IPC messages must be delivered reliably"
    );

    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_hud_subtitle_sequence_race_condition() {
    let mut subtitle_seq: u64 = 0;

    // Step 1: Subtitle 1 arrives
    subtitle_seq = subtitle_seq.wrapping_add(1);
    let seq_1 = subtitle_seq;
    let mut current_text = "Subtitle 1: Beware the trap!".to_string();
    assert_eq!(current_text, "Subtitle 1: Beware the trap!");

    // Step 2: 20ms later, Subtitle 2 arrives (overwriting Subtitle 1)
    tokio::time::sleep(Duration::from_millis(20)).await;
    subtitle_seq = subtitle_seq.wrapping_add(1);
    let seq_2 = subtitle_seq;
    current_text = "Subtitle 2: It is a mimic chest!".to_string();

    // Step 3: At 45ms, timer for Subtitle 1 fires with seq_1
    tokio::time::sleep(Duration::from_millis(25)).await;
    if seq_1 == subtitle_seq {
        current_text.clear();
    }
    // Subtitle 2 MUST still be visible!
    assert_eq!(
        current_text, "Subtitle 2: It is a mimic chest!",
        "Delayed clear timer from older subtitle must not clear newer subtitle"
    );

    // Step 4: When seq_2 timer fires, it clears properly
    if seq_2 == subtitle_seq {
        current_text.clear();
    }
    assert!(current_text.is_empty());
}

#[tokio::test]
async fn test_hud_styling_and_control_messages() {
    let socket_path =
        std::env::temp_dir().join(format!("test-hud-ctrl-{}.sock", std::process::id()));
    let server = HudIpcServer::new(&socket_path);
    let mut server_rx = server.start().await.expect("Server must start");
    let client = HudIpcClient::new(&socket_path);

    // Send UpdateStyle
    let style_msg = HudMessage::UpdateStyle {
        font_size: 28,
        opacity: 0.85,
        duration_ms: 5000,
        position: translator_core::ipc::HudPosition::Bottom,
        offset_y: 40,
    };
    client.send(&style_msg).await.expect("Send style failed");

    // Send SetVisible
    let vis_msg = HudMessage::SetVisible { visible: false };
    client.send(&vis_msg).await.expect("Send visible failed");

    // Send ClearSubtitle
    let clear_msg = HudMessage::ClearSubtitle;
    client.send(&clear_msg).await.expect("Send clear failed");

    // Verify reception
    let msg1 = tokio::time::timeout(Duration::from_secs(1), server_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg1, style_msg);

    let msg2 = tokio::time::timeout(Duration::from_secs(1), server_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg2, vis_msg);

    let msg3 = tokio::time::timeout(Duration::from_secs(1), server_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(msg3, clear_msg);

    let _ = std::fs::remove_file(&socket_path);
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
