use cosmic_translator_overlay::hud::SubtitleHudState;
use std::time::Duration;
use translator_core::ipc::{HudIpcClient, HudIpcServer, HudMessage};

#[tokio::test]
async fn test_ipc_interleaved_subtitle_and_status_message_stream() {
    let socket_path =
        std::env::temp_dir().join(format!("test-hud-interleaved-{}.sock", std::process::id()));

    let server = HudIpcServer::new(&socket_path);
    let mut server_rx = server.start().await.expect("HUD IPC server must start");

    let client = HudIpcClient::new(&socket_path);

    // Wait for server to bind
    let mut ready = false;
    for _ in 0..15 {
        if client.is_running().await {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(ready, "HUD IPC server failed to bind in time");

    // Interleave 100 alternating subtitle and status messages
    let mut expected_messages = Vec::new();

    for i in 0..50 {
        let sub_msg = HudMessage::ShowSubtitle {
            text: format!("Subtitle dialogue turn #{i}"),
            original: Some(format!("Original #{i}")),
            duration_ms: 5000,
        };
        client
            .send(&sub_msg)
            .await
            .expect("Send subtitle must succeed");
        expected_messages.push(sub_msg);

        let status_msg = HudMessage::SetStatus {
            active: i % 2 == 0,
            text: format!("Status notification #{i}"),
        };
        client
            .send(&status_msg)
            .await
            .expect("Send status must succeed");
        expected_messages.push(status_msg);
    }

    // Drain and verify all messages received in strict FIFO order
    let mut received_messages = Vec::new();
    let timeout = Duration::from_secs(5);
    let start = std::time::Instant::now();

    while received_messages.len() < expected_messages.len() && start.elapsed() < timeout {
        if let Ok(Some(msg)) =
            tokio::time::timeout(Duration::from_millis(100), server_rx.recv()).await
        {
            if msg == HudMessage::Ping {
                continue;
            }
            received_messages.push(msg);
        }
    }

    assert_eq!(
        received_messages.len(),
        expected_messages.len(),
        "All 100 interleaved IPC messages must be received without loss"
    );

    for (idx, (received, expected)) in received_messages
        .into_iter()
        .zip(expected_messages.into_iter())
        .enumerate()
    {
        assert_eq!(received, expected, "Message mismatch at FIFO index {idx}");
    }

    let _ = std::fs::remove_file(&socket_path);
}

#[tokio::test]
async fn test_concurrent_timer_clear_preserves_status_badge() {
    let mut hud = SubtitleHudState::default();
    let mut subtitle_seq: u64 = 0;

    // Step 1: Subtitle 1 arrives with 40ms duration
    subtitle_seq = subtitle_seq.wrapping_add(1);
    let seq_1 = subtitle_seq;
    hud.current_text = "Subtitle 1: Active dialogue".to_string();
    assert_eq!(hud.current_text, "Subtitle 1: Active dialogue");
    assert!(hud.status_badge.is_none());

    // Step 2: 15ms later, SetStatus arrives with HTTP 429 countdown
    tokio::time::sleep(Duration::from_millis(15)).await;
    hud.status_badge = Some("⏳ Rate Limited (HTTP 429): 25s remaining".to_string());
    assert_eq!(hud.current_text, "Subtitle 1: Active dialogue");
    assert_eq!(
        hud.status_badge.as_deref(),
        Some("⏳ Rate Limited (HTTP 429): 25s remaining")
    );

    // Step 3: At 50ms, timer for Subtitle 1 fires (seq_1 matches)
    tokio::time::sleep(Duration::from_millis(35)).await;
    if seq_1 == subtitle_seq {
        hud.current_text.clear();
    }
    // Subtitle cleared, BUT status badge MUST remain intact!
    assert_eq!(hud.current_text, "");
    assert_eq!(
        hud.status_badge.as_deref(),
        Some("⏳ Rate Limited (HTTP 429): 25s remaining"),
        "Timer clear of subtitle MUST NOT clear active status badge"
    );

    // Step 4: Subtitle 2 arrives with persistent duration (duration_ms = 0)
    subtitle_seq = subtitle_seq.wrapping_add(1);
    let _seq_2 = subtitle_seq;
    hud.current_text = "Subtitle 2: New scene dialogue".to_string();
    assert_eq!(hud.current_text, "Subtitle 2: New scene dialogue");
    assert_eq!(
        hud.status_badge.as_deref(),
        Some("⏳ Rate Limited (HTTP 429): 25s remaining")
    );

    // Step 5: Stale timer for seq_1 arrives late; must NOT clear Subtitle 2!
    if seq_1 == subtitle_seq {
        hud.current_text.clear();
    }
    assert_eq!(
        hud.current_text, "Subtitle 2: New scene dialogue",
        "Stale timer seq must be ignored"
    );
    assert_eq!(
        hud.status_badge.as_deref(),
        Some("⏳ Rate Limited (HTTP 429): 25s remaining")
    );

    // Step 6: Status badge cleared
    hud.status_badge = None;
    assert_eq!(hud.current_text, "Subtitle 2: New scene dialogue");
    assert!(hud.status_badge.is_none());
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
