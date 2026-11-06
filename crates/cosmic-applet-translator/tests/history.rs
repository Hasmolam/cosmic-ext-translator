use cosmic_applet_translator::history::{DialogueBacklog, HistoryEntry};

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
        pair_backlog.push(None, format!("Line_{i}"), format!("Tr_{i}"));
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
    let deserialized: HistoryEntry =
        serde_json::from_str(&serialized).expect("Deserialization must succeed");

    assert_eq!(entry, deserialized);
    assert_eq!(deserialized.id, 42);
    assert_eq!(deserialized.timestamp_str, "14:25:30");
    assert_eq!(deserialized.speaker, Some("Gözcü".to_string()));
    assert_eq!(deserialized.source, "Hedef ufukta göründü.");
    assert_eq!(deserialized.target, "Target appeared on the horizon.");
}

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
            if i % 2 == 0 {
                Some(format!("Hero {i}"))
            } else {
                None
            },
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
    backlog.push(Some("勇者".into()), "覚悟しろ！".into(), "Hazır ol!".into());

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
