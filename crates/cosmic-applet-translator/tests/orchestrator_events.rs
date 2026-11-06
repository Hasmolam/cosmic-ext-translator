use cosmic_applet_translator::history::DialogueBacklog;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::time::timeout;
use translator_core::orchestrator::OrchestratorEvent;
use translator_core::translation::parse_speaker_tags;

#[tokio::test]
async fn test_orchestrator_channel_rapid_dispatch_500_events_no_deadlock() {
    // Standard orchestrator channel capacity is 64 (crates/translator-core/src/orchestrator/mod.rs:95)
    let channel_capacity = 64;
    let (event_tx, mut event_rx) = mpsc::channel::<OrchestratorEvent>(channel_capacity);
    let total_events = 600; // > 500 events to aggressively exceed 64-capacity channel buffer

    let producer = tokio::spawn(async move {
        for i in 1..=total_events {
            let event = if i % 2 == 0 {
                OrchestratorEvent::TextDetected {
                    raw: format!("Detected frame text #{i}"),
                }
            } else {
                OrchestratorEvent::TranslationReady {
                    original: format!("Source dialogue #{i}"),
                    translated: format!("Çeviri metni #{i}"),
                    cached: i % 4 == 0,
                }
            };

            // If the drain halts or deadlocks, this send will block forever
            if let Err(err) = event_tx.send(event).await {
                panic!("Failed to send event #{i}: {err}");
            }
        }
    });

    let drain = tokio::spawn(async move {
        let mut count = 0;
        let mut detected_count = 0;
        let mut translated_count = 0;

        while count < total_events {
            if let Some(event) = event_rx.recv().await {
                count += 1;
                match event {
                    OrchestratorEvent::TextDetected { raw } => {
                        assert!(raw.starts_with("Detected frame text #"));
                        detected_count += 1;
                    }
                    OrchestratorEvent::TranslationReady {
                        original,
                        translated,
                        ..
                    } => {
                        assert!(original.starts_with("Source dialogue #"));
                        assert!(translated.starts_with("Çeviri metni #"));
                        translated_count += 1;
                    }
                    _ => {}
                }
            } else {
                break;
            }
        }

        (count, detected_count, translated_count)
    });

    // Enforce strict 5-second deadline. If deadlock occurs, timeout will trigger and fail test.
    let timeout_result = timeout(Duration::from_secs(5), async {
        let (producer_res, drain_res) = tokio::join!(producer, drain);
        producer_res.expect("Producer completed successfully without deadlock");
        drain_res.expect("Drain completed successfully")
    })
    .await;

    assert!(
        timeout_result.is_ok(),
        "Rapid 600-event dispatch must finish well within 5s deadline without deadlock"
    );
    let (total_received, detected, translated) = timeout_result.unwrap();
    assert_eq!(
        total_received, 600,
        "Must successfully receive exactly 600 events through 64-capacity channel"
    );
    assert_eq!(detected, 300);
    assert_eq!(translated, 300);
}

#[tokio::test]
async fn test_orchestrator_channel_concurrent_multi_producers_1000_events_deadlock_free() {
    let (event_tx, mut event_rx) = mpsc::channel::<OrchestratorEvent>(64);
    let producers_count = 4;
    let events_per_producer = 250;
    let total_events = producers_count * events_per_producer; // 1,000 total events

    let mut handles = Vec::new();
    for p_id in 0..producers_count {
        let tx = event_tx.clone();
        handles.push(tokio::spawn(async move {
            for i in 1..=events_per_producer {
                let event = OrchestratorEvent::TextDetected {
                    raw: format!("Producer_{p_id}_event_{i}"),
                };
                tx.send(event)
                    .await
                    .expect("Concurrent send must not fail or deadlock");
            }
        }));
    }
    // Drop the initial sender so channel closes when all tasks finish
    drop(event_tx);

    let drain = tokio::spawn(async move {
        let mut backlog = DialogueBacklog::default();
        let mut received = 0;
        while let Some(event) = event_rx.recv().await {
            received += 1;
            if let OrchestratorEvent::TextDetected { raw } = event {
                backlog.push(None, raw, "Translated".to_string());
            }
        }
        (received, backlog.entries().len())
    });

    let timeout_result = timeout(Duration::from_secs(5), async {
        for h in handles {
            h.await.expect("Producer task finished cleanly");
        }
        drain.await.expect("Drain task finished cleanly")
    })
    .await;

    assert!(
        timeout_result.is_ok(),
        "1,000 concurrent events must drain without deadlock"
    );
    let (received, backlog_len) = timeout_result.unwrap();
    assert_eq!(received, total_events);
    assert_eq!(
        backlog_len, 500,
        "DialogueBacklog fed from channel must cap at exactly 500 entries"
    );
}

#[tokio::test]
async fn test_broadcast_event_bus_overflow_resilience() {
    // Simulates cosmic-applet-translator's internal broadcast EVENT_BUS (capacity 256)
    let (bus_tx, mut bus_rx) = broadcast::channel::<OrchestratorEvent>(256);

    // 1. Sending with zero active subscribers does NOT block or panic
    let (lonely_tx, _lonely_rx) = broadcast::channel::<OrchestratorEvent>(16);
    drop(_lonely_rx); // Zero subscribers
    for i in 1..=50 {
        let res = lonely_tx.send(OrchestratorEvent::TextDetected {
            raw: format!("Unheard event #{i}"),
        });
        // broadcast::send returns Err when there are no subscribers, but crucially NEVER blocks
        assert!(res.is_err());
    }

    // 2. Slow consumer lagged handling
    let total_flood = 600;
    for i in 1..=total_flood {
        let _ = bus_tx.send(OrchestratorEvent::TextDetected {
            raw: format!("Flood event #{i}"),
        });
    }

    // Consumer reads after flood: it should encounter RecvError::Lagged and recover
    match bus_rx.recv().await {
        Err(broadcast::error::RecvError::Lagged(missed)) => {
            // Buffer size is 256, so it missed 600 - 256 = 344 events
            assert!(missed > 0, "Lagged receiver must report missed messages");
        }
        Ok(event) => panic!("Expected Lagged error, got event {event:?}"),
        Err(e) => panic!("Unexpected error {e}"),
    }

    // Next recv succeeds with the latest surviving buffered event
    let next_event = bus_rx
        .recv()
        .await
        .expect("Should receive next buffered message");
    if let OrchestratorEvent::TextDetected { raw } = next_event {
        assert!(raw.starts_with("Flood event #"));
    } else {
        panic!("Unexpected event type");
    }
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
        OrchestratorEvent::TranslationReady {
            original,
            translated,
            ..
        } => {
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
        OrchestratorEvent::TranslationReady {
            original,
            translated,
            ..
        } => {
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
        OrchestratorEvent::TranslationReady {
            original,
            translated,
            ..
        } => {
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
    assert_eq!(
        entry3.source,
        "A thunderous roar echoed through the valley."
    );
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
            event_tx
                .send(event)
                .await
                .expect("Channel must not deadlock");
        }
    });

    let mut received = 0;
    while received < total_events {
        if let Some(OrchestratorEvent::TextDetected { raw }) = event_rx.recv().await {
            assert!(raw.starts_with("Detected frame #"));
            received += 1;
        }
    }

    producer
        .await
        .expect("Producer task completed successfully");
    assert_eq!(
        received, total_events,
        "All 200 events successfully drained through 64-capacity channel"
    );
}
