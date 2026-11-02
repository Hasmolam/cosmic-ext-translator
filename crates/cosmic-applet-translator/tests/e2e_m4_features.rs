use cosmic_applet_translator::config::Config;
use cosmic_applet_translator::history::DialogueBacklog;
use cosmic_applet_translator::settings_dialog::{SettingsState, SettingsTab};
use std::collections::HashMap;
use translator_core::capture::roi::Roi;
use translator_core::orchestrator::OrchestratorEvent;
use translator_core::translation::parse_speaker_tags;

#[test]
fn test_backlog_boundary_capacities_and_fifo() {
    // 1. Capacity 1 edge case
    let mut backlog = DialogueBacklog::new(1);
    backlog.push(Some("A".into()), "First".into(), "İlk".into());
    assert_eq!(backlog.entries().len(), 1);
    assert_eq!(backlog.entries()[0].source, "First");

    backlog.push(Some("B".into()), "Second".into(), "İkinci".into());
    assert_eq!(backlog.entries().len(), 1);
    assert_eq!(backlog.entries()[0].source, "Second");

    // 2. Capacity 500 edge case
    let mut backlog500 = DialogueBacklog::new(500);
    for i in 1..=550 {
        backlog500.push(
            if i % 2 == 0 { Some(format!("Hero {i}")) } else { None },
            format!("Source line {i}"),
            format!("Hedef satır {i}"),
        );
    }
    assert_eq!(backlog500.entries().len(), 500);
    // Newest is at index 0
    assert_eq!(backlog500.entries()[0].source, "Source line 550");
    // Oldest surviving is at index 499 (line 51)
    assert_eq!(backlog500.entries()[499].source, "Source line 51");
}

#[test]
fn test_backlog_cjk_and_multiline_support() {
    let mut backlog = DialogueBacklog::new(10);
    backlog.push(
        Some("魔王".into()),
        "我が城へようこそ。\n命を捨てに来たか？".into(),
        "Kaleme hoş geldin.\nCanını vermeye mi geldin?".into(),
    );
    backlog.push(
        Some("勇者".into()),
        "覚悟しろ！".into(),
        "Hazır ol!".into(),
    );

    assert_eq!(backlog.entries().len(), 2);

    let search_cjk = backlog.search("城へようこそ");
    assert_eq!(search_cjk.len(), 1);
    assert_eq!(search_cjk[0].speaker, Some("魔王".to_string()));

    let search_speaker = backlog.search("勇者");
    assert_eq!(search_speaker.len(), 1);
    assert_eq!(search_speaker[0].target, "Hazır ol!");

    let search_multiline = backlog.search("Canını");
    assert_eq!(search_multiline.len(), 1);
}

#[test]
fn test_orchestrator_event_speaker_parsing_and_backlog_pipeline() {
    let glossary = HashMap::from([
        ("魔王".to_string(), "Demon Lord".to_string()),
        ("Hero".to_string(), "Kahraman".to_string()),
    ]);

    let mut backlog = DialogueBacklog::default();

    // Simulate OrchestratorEvent::TranslationReady with Lenticular brackets
    let event1 = OrchestratorEvent::TranslationReady {
        original: "【魔王】貴様の力はその程度か？".to_string(),
        translated: "Gücün bu kadar mı?".to_string(),
        cached: false,
    };

    match event1 {
        OrchestratorEvent::TranslationReady { original, translated, .. } => {
            let parsed = parse_speaker_tags(&original, &glossary);
            let speaker = parsed.resolved_speaker.or(parsed.raw_speaker);
            let source_text = if !parsed.dialogue.is_empty() {
                parsed.dialogue
            } else {
                original.clone()
            };
            backlog.push(speaker, source_text, translated);
        }
        _ => panic!("Expected TranslationReady"),
    }

    assert_eq!(backlog.entries().len(), 1);
    let entry1 = &backlog.entries()[0];
    assert_eq!(entry1.speaker, Some("Demon Lord".to_string()));
    assert_eq!(entry1.source, "貴様の力はその程度か？");
    assert_eq!(entry1.target, "Gücün bu kadar mı?");

    // Simulate OrchestratorEvent::TranslationReady with standard brackets
    let event2 = OrchestratorEvent::TranslationReady {
        original: "[Hero] Never give up!".to_string(),
        translated: "Asla pes etme!".to_string(),
        cached: true,
    };

    match event2 {
        OrchestratorEvent::TranslationReady { original, translated, .. } => {
            let parsed = parse_speaker_tags(&original, &glossary);
            let speaker = parsed.resolved_speaker.or(parsed.raw_speaker);
            let source_text = if !parsed.dialogue.is_empty() {
                parsed.dialogue
            } else {
                original.clone()
            };
            backlog.push(speaker, source_text, translated);
        }
        _ => panic!("Expected TranslationReady"),
    }

    assert_eq!(backlog.entries().len(), 2);
    let entry2 = &backlog.entries()[0];
    assert_eq!(entry2.speaker, Some("Kahraman".to_string()));
    assert_eq!(entry2.source, "Never give up!");
    assert_eq!(entry2.target, "Asla pes etme!");

    // Simulate unbracketed narration line
    let event3 = OrchestratorEvent::TranslationReady {
        original: "A thunderous roar echoed through the valley.".to_string(),
        translated: "Vadide gök gürültüsü gibi bir kükreme yankılandı.".to_string(),
        cached: false,
    };

    match event3 {
        OrchestratorEvent::TranslationReady { original, translated, .. } => {
            let parsed = parse_speaker_tags(&original, &glossary);
            let speaker = parsed.resolved_speaker.or(parsed.raw_speaker);
            let source_text = if !parsed.dialogue.is_empty() {
                parsed.dialogue
            } else {
                original.clone()
            };
            backlog.push(speaker, source_text, translated);
        }
        _ => panic!("Expected TranslationReady"),
    }

    assert_eq!(backlog.entries().len(), 3);
    let entry3 = &backlog.entries()[0];
    assert!(entry3.speaker.is_none());
    assert_eq!(entry3.source, "A thunderous roar echoed through the valley.");
}

#[tokio::test]
async fn test_orchestrator_channel_deadlock_prevention() {
    // Tests that sending > 64 events (the channel buffer capacity) does not stall
    // when actively drained.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<OrchestratorEvent>(64);
    let total_events = 200;

    let producer = tokio::spawn(async move {
        for i in 0..total_events {
            let event = OrchestratorEvent::TextDetected {
                raw: format!("Detected frame #{i}"),
            };
            // If the buffer stalls, this send will deadlock and time out
            event_tx.send(event).await.expect("Channel must not deadlock");
        }
    });

    let mut received = 0;
    while received < total_events {
        if let Some(OrchestratorEvent::TextDetected { raw }) = event_rx.recv().await {
            assert!(raw.starts_with("Detected frame #"));
            received += 1;
        }
    }

    producer.await.expect("Producer task completed successfully");
    assert_eq!(received, total_events, "All 200 events successfully drained through 64-capacity channel");
}

#[test]
fn test_settings_dialog_history_and_ignore_roi_integration() {
    cosmic_applet_translator::i18n::init(&[]);
    let mut config = Config::default();
    let mut backlog = DialogueBacklog::new(100);

    for i in 1..=10 {
        backlog.push(
            Some(format!("NPC_{i}")),
            format!("Quest instruction {i}"),
            format!("Görev talimatı {i}"),
        );
    }

    let mut state = SettingsState::new();

    // 1. History Tab Navigation
    state.active_tab = SettingsTab::History;
    let _ = state.view(&config, &backlog);

    // 2. Search filtering
    state.history_search_query = "NPC_3".to_string();
    assert_eq!(backlog.search(&state.history_search_query).len(), 1);
    let _ = state.view(&config, &backlog);

    // 3. Clear History
    backlog.clear();
    assert!(backlog.entries().is_empty());
    let _ = state.view(&config, &backlog);

    // 4. Capture Tab Ignore ROI controls
    state.active_tab = SettingsTab::Capture;
    config.ignore_roi = Some(Roi::new(10, 20, 200, 150));
    let _ = state.view(&config, &backlog);

    config.ignore_roi = None;
    let _ = state.view(&config, &backlog);
}
