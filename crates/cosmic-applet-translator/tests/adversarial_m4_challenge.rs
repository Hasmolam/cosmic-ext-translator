use cosmic_applet_translator::history::{DialogueBacklog, HistoryEntry, DEFAULT_MAX_HISTORY_ENTRIES};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tokio::time::timeout;
use translator_core::orchestrator::OrchestratorEvent;
use translator_core::translation::{parse_speaker_tags, BracketStyle};

// =============================================================================
// PART 1: DialogueBacklog Capacity Bounds & FIFO Eviction Stress Tests
// =============================================================================

#[test]
fn test_stress_insertion_rapid_1500_entries_exact_500_fifo_bounds() {
    let mut backlog = DialogueBacklog::default();
    assert_eq!(backlog.max_entries, DEFAULT_MAX_HISTORY_ENTRIES);
    assert_eq!(backlog.max_entries, 500);
    assert!(backlog.entries().is_empty());

    // Rapidly push 1,500 entries (>1,000 as mandated by dispatch)
    for i in 1..=1500 {
        backlog.push(
            Some(format!("Speaker_{i}")),
            format!("Source dialogue #{i}"),
            format!("Target translation #{i}"),
        );

        let current_len = backlog.entries().len();
        if i <= 500 {
            assert_eq!(current_len, i, "Backlog length must equal insertion count prior to reaching capacity");
        } else {
            assert_eq!(current_len, 500, "Backlog length must be strictly capped at 500 once capacity is reached");
        }
    }

    // Verify exact bound
    assert_eq!(backlog.entries().len(), 500);

    // Verify newest entry (index 0) is item #1500
    let newest = &backlog.entries()[0];
    assert_eq!(newest.id, 1500);
    assert_eq!(newest.speaker, Some("Speaker_1500".to_string()));
    assert_eq!(newest.source, "Source dialogue #1500");
    assert_eq!(newest.target, "Target translation #1500");

    // Verify oldest surviving entry (index 499) is item #1001 (1500 - 500 + 1)
    let oldest = &backlog.entries()[499];
    assert_eq!(oldest.id, 1001);
    assert_eq!(oldest.speaker, Some("Speaker_1001".to_string()));
    assert_eq!(oldest.source, "Source dialogue #1001");
    assert_eq!(oldest.target, "Target translation #1001");

    // Verify strict FIFO ordering across the entire 500-element window:
    // Every index k must correspond exactly to item #(1500 - k)
    for (k, entry) in backlog.entries().iter().enumerate() {
        let expected_num = 1500 - k;
        assert_eq!(entry.id, expected_num as u64);
        assert_eq!(entry.speaker, Some(format!("Speaker_{expected_num}")));
        assert_eq!(entry.source, format!("Source dialogue #{expected_num}"));
        assert_eq!(entry.target, format!("Target translation #{expected_num}"));
    }

    // Push 500 more items (total 2,000 inserted)
    for i in 1501..=2000 {
        backlog.push(
            None,
            format!("Narration #{i}"),
            format!("Anlatı #{i}"),
        );
        assert_eq!(backlog.entries().len(), 500);
    }

    assert_eq!(backlog.entries()[0].source, "Narration #2000");
    assert_eq!(backlog.entries()[499].source, "Narration #1501");
}

#[test]
fn test_backlog_edge_capacities_and_clear_cycle() {
    // 1. Capacity = 1: Single-entry sliding window
    let mut single_backlog = DialogueBacklog::new(1);
    for i in 1..=200 {
        single_backlog.push(
            Some(format!("S_{i}")),
            format!("Text_{i}"),
            format!("Hedef_{i}"),
        );
        assert_eq!(single_backlog.entries().len(), 1);
        assert_eq!(single_backlog.entries()[0].source, format!("Text_{i}"));
        assert_eq!(single_backlog.entries()[0].id, i);
    }

    // 2. Capacity = 2: Double-entry sliding window
    let mut pair_backlog = DialogueBacklog::new(2);
    for i in 1..=200 {
        pair_backlog.push(
            None,
            format!("Line_{i}"),
            format!("Tr_{i}"),
        );
        if i == 1 {
            assert_eq!(pair_backlog.entries().len(), 1);
        } else {
            assert_eq!(pair_backlog.entries().len(), 2);
            assert_eq!(pair_backlog.entries()[0].source, format!("Line_{i}"));
            assert_eq!(pair_backlog.entries()[1].source, format!("Line_{}", i - 1));
        }
    }

    // 3. Clear cycle: fill to capacity, clear, and fill again
    let mut cycle_backlog = DialogueBacklog::default();
    for i in 1..=600 {
        cycle_backlog.push(None, format!("Batch1_{i}"), format!("B1_{i}"));
    }
    assert_eq!(cycle_backlog.entries().len(), 500);

    cycle_backlog.clear();
    assert_eq!(cycle_backlog.entries().len(), 0);
    assert!(cycle_backlog.entries().is_empty());

    for i in 1..=700 {
        cycle_backlog.push(None, format!("Batch2_{i}"), format!("B2_{i}"));
    }
    assert_eq!(cycle_backlog.entries().len(), 500);
    assert_eq!(cycle_backlog.entries()[0].source, "Batch2_700");
    assert_eq!(cycle_backlog.entries()[499].source, "Batch2_201");
    // Verify IDs continued incrementing monotonically (batch 1 used 1..=600, batch 2 uses 601..=1300)
    assert_eq!(cycle_backlog.entries()[0].id, 1300);
    assert_eq!(cycle_backlog.entries()[499].id, 801);
}

// =============================================================================
// PART 2: Search Query Filtering Stress Tests (Unicode, Emojis, Whitespace, etc.)
// =============================================================================

#[test]
fn test_search_filtering_multilingual_unicode_scripts() {
    let mut backlog = DialogueBacklog::new(50);

    backlog.push(
        Some("魔王".to_string()),
        "我が城へようこそ、愚かな勇者よ。".to_string(),
        "Şatoma hoş geldin, aptal kahraman.".to_string(),
    );
    backlog.push(
        Some("アリス".to_string()),
        "不思議の国のアリスは時計を見た。".to_string(),
        "Alice Harikalar Diyarında saatine baktı.".to_string(),
    );
    backlog.push(
        Some("Valkyrie".to_string()),
        "戦乙女のヴァルキリーが剣を構えた。".to_string(),
        "Savaş bakiresi Valkyrie kılıcını çekti.".to_string(),
    );
    backlog.push(
        Some("Muhafız".to_string()),
        "Şehrin ışıkları söndü, karanlık çöktü.".to_string(),
        "City lights dimmed, darkness fell.".to_string(),
    );
    backlog.push(
        Some("Gezgin".to_string()),
        "İSTANBUL sokaklarında ÇAY içiyorum.".to_string(),
        "Drinking tea in Istanbul streets.".to_string(),
    );
    backlog.push(
        Some("Иван".to_string()),
        "Здравствуй, мир! Славная победа.".to_string(),
        "Hello, world! Glorious victory.".to_string(),
    );
    backlog.push(
        Some("König".to_string()),
        "Große weiße Schlösser in den Bergen.".to_string(),
        "Dağlardaki büyük beyaz kaleler.".to_string(),
    );
    backlog.push(
        Some("مسافر".to_string()),
        "مرحبا بكم في واحة السلام".to_string(),
        "Barış vahasına hoş geldiniz".to_string(),
    );

    // 1. Japanese Kanji & Kana queries
    assert_eq!(backlog.search("魔王").len(), 1);
    assert_eq!(backlog.search("ようこそ").len(), 1);
    assert_eq!(backlog.search("アリス").len(), 1);
    assert_eq!(backlog.search("ヴァルキリー").len(), 1);

    // 2. Turkish dotted/dotless I & diacritics
    // "ışıkları" matched with lowercase "ışık"
    let turkish_res = backlog.search("ışık");
    assert_eq!(turkish_res.len(), 1);
    assert_eq!(turkish_res[0].speaker, Some("Muhafız".to_string()));

    // "ÇAY" matched with case-folded "çay"
    let cay_res = backlog.search("çay");
    assert_eq!(cay_res.len(), 1);
    assert_eq!(cay_res[0].speaker, Some("Gezgin".to_string()));

    // 3. Cyrillic queries
    assert_eq!(backlog.search("здравствуй").len(), 1);
    assert_eq!(backlog.search("ПОБЕДА").len(), 1);
    assert_eq!(backlog.search("Иван").len(), 1);

    // 4. German umlaut & eszett queries
    assert_eq!(backlog.search("große").len(), 1);
    assert_eq!(backlog.search("schlösser").len(), 1);
    assert_eq!(backlog.search("könig").len(), 1);

    // 5. Arabic script queries
    assert_eq!(backlog.search("مرحبا").len(), 1);
    assert_eq!(backlog.search("السلام").len(), 1);
    assert_eq!(backlog.search("مسافر").len(), 1);

    // 6. Non-matching query returns empty Vec
    assert!(backlog.search("nonexistent_script_xyz").is_empty());
}

#[test]
fn test_search_filtering_emojis_and_complex_zwj() {
    let mut backlog = DialogueBacklog::new(20);

    backlog.push(
        Some("🧙‍♂️ Wizard".to_string()),
        "Casting ultimate spell: 🔥✨ Meteor Storm!".to_string(),
        "Nihai büyü yapılıyor: 🔥✨ Meteor Fırtınası!".to_string(),
    );
    backlog.push(
        Some("🛡️ Knight".to_string()),
        "Protecting the kingdom family 👨‍👩‍👧‍👦 at all costs!".to_string(),
        "Krallık ailesini 👨‍👩‍👧‍👦 ne pahasına olursa olsun korumak!".to_string(),
    );
    backlog.push(
        None,
        "Player defeated. 💀 Game Over. Press 🔄 to restart.".to_string(),
        "Oyuncu yenildi. 💀 Oyun Bitti. Yeniden başlatmak için 🔄 basın.".to_string(),
    );
    backlog.push(
        Some("Diplomat".to_string()),
        "Flight path: 🇯🇵 Tokyo -> 🇹🇷 Istanbul".to_string(),
        "Uçuş rotası: 🇯🇵 Tokyo -> 🇹🇷 İstanbul".to_string(),
    );

    // Emoji searches
    assert_eq!(backlog.search("🔥").len(), 1);
    assert_eq!(backlog.search("✨").len(), 1);
    assert_eq!(backlog.search("👨‍👩‍👧‍👦").len(), 1);
    assert_eq!(backlog.search("💀").len(), 1);
    assert_eq!(backlog.search("🔄").len(), 1);
    assert_eq!(backlog.search("🇯🇵").len(), 1);
    assert_eq!(backlog.search("🇹🇷").len(), 1);

    // Cross-field emoji searches (speaker vs source vs target)
    assert_eq!(backlog.search("🧙‍♂️").len(), 1);
    assert_eq!(backlog.search("🛡️").len(), 1);
    assert_eq!(backlog.search("Meteor").len(), 1);
}

#[test]
fn test_search_filtering_whitespace_control_chars_and_regex_metachars() {
    let mut backlog = DialogueBacklog::new(20);

    backlog.push(
        Some("Hero [Level 99]".to_string()),
        "Regex test: (.*) +? ^$ {val} | [tag] \\path".to_string(),
        "Regex testi: (.*) +? ^$ {val} | [tag] \\path".to_string(),
    );
    backlog.push(
        None,
        "Embedded control chars: \x1b[31mRed\x1b[0m and \t tabs \r\n newlines".to_string(),
        "Kontrol karakterleri: \x1b[31mKırmızı\x1b[0m".to_string(),
    );
    backlog.push(
        Some("NullByte".to_string()),
        "Contains null: hello\0world".to_string(),
        "Null içerir: merhaba\0dünya".to_string(),
    );

    // Whitespace only queries must return all 3 entries without panic
    assert_eq!(backlog.search("").len(), 3);
    assert_eq!(backlog.search("   ").len(), 3);
    assert_eq!(backlog.search("\t\t").len(), 3);
    assert_eq!(backlog.search("\r\n\r\n").len(), 3);

    // Leading and trailing whitespace should be trimmed and match target
    assert_eq!(backlog.search("  regex test  ").len(), 1);
    assert_eq!(backlog.search("   hero [level 99]   ").len(), 1);

    // Regex metacharacters must be matched literally, not as patterns
    assert_eq!(backlog.search("(.*)").len(), 1);
    assert_eq!(backlog.search("+?").len(), 1);
    assert_eq!(backlog.search("^$").len(), 1);
    assert_eq!(backlog.search("{val}").len(), 1);
    assert_eq!(backlog.search("[tag]").len(), 1);
    assert_eq!(backlog.search("\\path").len(), 1);

    // Unmatched special chars do not panic
    assert!(!backlog.search("(").is_empty());
    assert!(!backlog.search("[").is_empty());
    assert!(!backlog.search("\\").is_empty());
    assert!(backlog.search("?*+").is_empty());

    // Null bytes and ANSI escapes
    assert_eq!(backlog.search("\0").len(), 1);
    assert_eq!(backlog.search("\x1b[31m").len(), 1);

    // Extremely long search query (10,000 chars) must return empty cleanly
    let long_query = "A".repeat(10_000);
    assert!(backlog.search(&long_query).is_empty());
}

// =============================================================================
// PART 3: Speaker Tag Attribution Corner Cases & Glossary Integration
// =============================================================================

#[test]
fn test_speaker_parsing_nested_brackets() {
    let mut glossary = HashMap::new();
    glossary.insert("Hero".to_string(), "Kahraman".to_string());
    glossary.insert("Mage".to_string(), "Büyücü".to_string());

    // Case 1: Japanese lenticular containing square brackets: 【[Hero]】 Dialogue
    let line1 = parse_speaker_tags("【[Hero]】 Let's move forward!", &glossary);
    assert_eq!(line1.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(line1.raw_speaker.as_deref(), Some("[Hero]"));
    assert_eq!(line1.dialogue, "Let's move forward!");

    // Case 2: Square bracket containing lenticular: [【Mage】] Cast spell
    let line2 = parse_speaker_tags("[【Mage】] Cast spell", &glossary);
    assert_eq!(line2.bracket_style, Some(BracketStyle::SquareBracket));
    assert_eq!(line2.raw_speaker.as_deref(), Some("【Mage】"));
    assert_eq!(line2.dialogue, "Cast spell");

    // Case 3: Double square brackets: [[Hero]] Double protected
    let line3 = parse_speaker_tags("[[Hero]] Double protected", &glossary);
    assert_eq!(line3.bracket_style, Some(BracketStyle::SquareBracket));
    // The inner [ is captured as part of speaker nameplate
    assert_eq!(line3.raw_speaker.as_deref(), Some("[Hero"));
    assert_eq!(line3.dialogue, "] Double protected");

    // Case 4: Bracket followed by colon delimiter: [Hero]: Roger that!
    let line4 = parse_speaker_tags("[Hero]: Roger that!", &glossary);
    assert_eq!(line4.bracket_style, Some(BracketStyle::SquareBracket));
    assert_eq!(line4.raw_speaker.as_deref(), Some("Hero"));
    assert_eq!(line4.resolved_speaker.as_deref(), Some("Kahraman"));
    assert_eq!(line4.dialogue, "Roger that!");

    // Case 5: Japanese fullwidth colon following lenticular: 【Mage】： 了解。
    let line5 = parse_speaker_tags("【Mage】： 了解。", &glossary);
    assert_eq!(line5.bracket_style, Some(BracketStyle::Lenticular));
    assert_eq!(line5.raw_speaker.as_deref(), Some("Mage"));
    assert_eq!(line5.resolved_speaker.as_deref(), Some("Büyücü"));
    assert_eq!(line5.dialogue, "了解。");
}

#[test]
fn test_speaker_parsing_unclosed_and_missing_brackets() {
    let glossary = HashMap::new();

    let unclosed_cases = [
        "【Missing close lenticular bracket and dialogue",
        "〔Missing close tortoise shell bracket and dialogue",
        "「Missing close corner bracket and dialogue",
        "[Missing close square bracket and dialogue",
        "［Missing close fullwidth square bracket and dialogue",
        "〖Missing close white lenticular bracket and dialogue",
    ];

    for unclosed in unclosed_cases {
        let parsed = parse_speaker_tags(unclosed, &glossary);
        // An unclosed bracket must NOT parse as a speaker tag
        assert!(
            parsed.bracket_style.is_none(),
            "Unclosed bracket must not have bracket_style for '{unclosed}'"
        );
        assert!(
            parsed.raw_speaker.is_none(),
            "Unclosed bracket must have raw_speaker == None for '{unclosed}'"
        );
        assert!(
            parsed.resolved_speaker.is_none(),
            "Unclosed bracket must have resolved_speaker == None for '{unclosed}'"
        );
        assert_eq!(
            parsed.dialogue, unclosed,
            "Dialogue must preserve the full string for unclosed bracket"
        );
    }
}

#[test]
fn test_speaker_parsing_empty_dialogue_and_corner_delimiters() {
    let mut glossary = HashMap::new();
    glossary.insert("Boss".to_string(), "Patron".to_string());

    // 1. Empty brackets: 【】 or []
    let empty_bracket1 = parse_speaker_tags("【】 Empty speaker here", &glossary);
    assert!(empty_bracket1.raw_speaker.is_none());
    assert_eq!(empty_bracket1.dialogue, "【】 Empty speaker here");

    let empty_bracket2 = parse_speaker_tags("[] Empty square speaker", &glossary);
    assert!(empty_bracket2.raw_speaker.is_none());
    assert_eq!(empty_bracket2.dialogue, "[] Empty square speaker");

    let whitespace_bracket = parse_speaker_tags("[   ] Whitespace speaker", &glossary);
    assert!(whitespace_bracket.raw_speaker.is_none());
    assert_eq!(whitespace_bracket.dialogue, "[   ] Whitespace speaker");

    // 2. Speaker tag without dialogue: 【Boss】
    // When speaker is in glossary, it should resolve speaker even with empty dialogue
    let boss_only = parse_speaker_tags("【Boss】", &glossary);
    assert_eq!(boss_only.raw_speaker.as_deref(), Some("Boss"));
    assert_eq!(boss_only.resolved_speaker.as_deref(), Some("Patron"));
    assert_eq!(boss_only.dialogue, "");

    // When speaker is NOT in glossary and dialogue is empty, it falls back to full string
    let unknown_only = parse_speaker_tags("【UnknownNPC】", &glossary);
    assert!(unknown_only.raw_speaker.is_none());
    assert_eq!(unknown_only.dialogue, "【UnknownNPC】");

    // 3. Colon delimiter corner cases
    // URL guard: http:// and https:// should NOT parse as speaker name
    let url_case = parse_speaker_tags("http://example.com/api", &glossary);
    assert!(url_case.raw_speaker.is_none());
    assert_eq!(url_case.dialogue, "http://example.com/api");

    // Timestamp guard: 12:34:56 should NOT parse as speaker name
    let time_case = parse_speaker_tags("12:34:56 system event", &glossary);
    assert!(time_case.raw_speaker.is_none());

    // Colon at pos 0: ": starting with colon"
    let zero_colon = parse_speaker_tags(": starting with colon", &glossary);
    assert!(zero_colon.raw_speaker.is_none());

    // Long prefix (>25 chars) before colon
    let long_prefix = "This prefix text is way too long to be a speaker name: Dialogue";
    let long_colon = parse_speaker_tags(long_prefix, &glossary);
    assert!(long_colon.raw_speaker.is_none());
    assert_eq!(long_colon.dialogue, long_prefix);

    // Multiline before colon:
    let multiline_colon = "First Line\nSecond Line: Dialogue";
    let multi_res = parse_speaker_tags(multiline_colon, &glossary);
    assert!(multi_res.raw_speaker.is_none());

    // Valid colon nameplate
    let valid_colon = parse_speaker_tags("Narrator: Once upon a time in a distant land...", &glossary);
    assert_eq!(valid_colon.bracket_style, Some(BracketStyle::ColonNameplate));
    assert_eq!(valid_colon.raw_speaker.as_deref(), Some("Narrator"));
    assert_eq!(valid_colon.dialogue, "Once upon a time in a distant land...");
}

// =============================================================================
// PART 4: Orchestrator Event Drain Channel Deadlock & Throughput Stress
// =============================================================================

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
                    OrchestratorEvent::TranslationReady { original, translated, .. } => {
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

    assert!(timeout_result.is_ok(), "Rapid 600-event dispatch must finish well within 5s deadline without deadlock");
    let (total_received, detected, translated) = timeout_result.unwrap();
    assert_eq!(total_received, 600, "Must successfully receive exactly 600 events through 64-capacity channel");
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
                tx.send(event).await.expect("Concurrent send must not fail or deadlock");
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

    assert!(timeout_result.is_ok(), "1,000 concurrent events must drain without deadlock");
    let (received, backlog_len) = timeout_result.unwrap();
    assert_eq!(received, total_events);
    assert_eq!(backlog_len, 500, "DialogueBacklog fed from channel must cap at exactly 500 entries");
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
    let next_event = bus_rx.recv().await.expect("Should receive next buffered message");
    if let OrchestratorEvent::TextDetected { raw } = next_event {
        assert!(raw.starts_with("Flood event #"));
    } else {
        panic!("Unexpected event type");
    }
}

// =============================================================================
// PART 5: HistoryEntry Serialization & Data Integrity
// =============================================================================

#[test]
fn test_history_entry_serde_roundtrip_and_equality() {
    let entry = HistoryEntry {
        id: 42,
        timestamp_str: "14:25:30".to_string(),
        speaker: Some("Gözcü".to_string()),
        source: "Hedef ufukta göründü.".to_string(),
        target: "Target appeared on the horizon.".to_string(),
    };

    let serialized = serde_json::to_string(&entry).expect("Serialization to JSON must succeed");
    let deserialized: HistoryEntry = serde_json::from_str(&serialized).expect("Deserialization must succeed");

    assert_eq!(entry, deserialized);
    assert_eq!(deserialized.id, 42);
    assert_eq!(deserialized.timestamp_str, "14:25:30");
    assert_eq!(deserialized.speaker, Some("Gözcü".to_string()));
    assert_eq!(deserialized.source, "Hedef ufukta göründü.");
    assert_eq!(deserialized.target, "Target appeared on the horizon.");
}
