use cosmic_applet_translator::config::Config;
use cosmic_applet_translator::history::{DialogueBacklog, DEFAULT_MAX_HISTORY_ENTRIES};
use cosmic_applet_translator::settings::{SettingsState, SettingsTab};
use translator_core::region::{Roi, RoiPreset};

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
            assert_eq!(
                current_len, i,
                "Backlog length must equal insertion count prior to reaching capacity"
            );
        } else {
            assert_eq!(
                current_len, 500,
                "Backlog length must be strictly capped at 500 once capacity is reached"
            );
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
        backlog.push(None, format!("Narration #{i}"), format!("Anlatı #{i}"));
        assert_eq!(backlog.entries().len(), 500);
    }

    assert_eq!(backlog.entries()[0].source, "Narration #2000");
    assert_eq!(backlog.entries()[499].source, "Narration #1501");
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
    state.history.query = "NPC_3".to_string();
    assert_eq!(backlog.search(&state.history.query).len(), 1);
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

#[test]
fn test_ui_settings_tab_transitions() {
    cosmic_applet_translator::i18n::init(&[]);
    let config = Config::default();
    let backlog = DialogueBacklog::default();
    let mut state = SettingsState::new();

    // Verify initial tab is Capture
    assert_eq!(state.active_tab, SettingsTab::Capture);
    let _ = state.view(&config, &backlog);

    // Switch to Engine tab
    state.active_tab = SettingsTab::Engine;
    assert_eq!(state.active_tab, SettingsTab::Engine);
    let _ = state.view(&config, &backlog);

    // Switch to Display tab
    state.active_tab = SettingsTab::Display;
    assert_eq!(state.active_tab, SettingsTab::Display);
    let _ = state.view(&config, &backlog);

    // Switch to Glossary tab
    state.active_tab = SettingsTab::Glossary;
    assert_eq!(state.active_tab, SettingsTab::Glossary);
    let _ = state.view(&config, &backlog);

    // Switch to History tab (5th tab)
    state.active_tab = SettingsTab::History;
    assert_eq!(state.active_tab, SettingsTab::History);
    let _ = state.view(&config, &backlog);
}

#[test]
fn test_ui_engine_views_render_cleanly() {
    cosmic_applet_translator::i18n::init(&[]);
    let mut config = Config::default();
    let backlog = DialogueBacklog::default();
    let mut state = SettingsState::new();
    state.active_tab = SettingsTab::Engine;

    // 1. Google
    config.engine = "google".to_string();
    let _ = state.view(&config, &backlog);

    // 2. DeepL
    config.engine = "deepl".to_string();
    config.deepl_api_key = "test-auth-key:fx".to_string();
    let _ = state.view(&config, &backlog);

    // 3. Ollama
    config.engine = "ollama".to_string();
    config.ollama_endpoint = "http://localhost:11434".to_string();
    config.ollama_model = "qwen2.5:3b".to_string();
    let _ = state.view(&config, &backlog);

    // 4. OpenAI
    config.engine = "openai".to_string();
    config.openai_endpoint = "https://api.openai.com/v1".to_string();
    config.openai_api_key = "sk-test-secret-key".to_string();
    config.openai_model = "gpt-4o-mini".to_string();
    let _ = state.view(&config, &backlog);
}

#[test]
fn test_ui_config_persistence_and_ron_roundtrip() {
    let cfg = Config {
        enabled: true,
        target_lang: "tr".to_string(),
        source_lang: "ja".to_string(),
        ocr_orientation: "vertical".to_string(),
        ocr_engine: "tesseract".to_string(),
        restore_token: Some("test-restore-token-abc".to_string()),
        preset: RoiPreset::Custom.as_str().to_string(),
        custom_roi: Some(Roi::new(120, 240, 600, 300)),
        ignore_roi: None,
        engine: "deepl".to_string(),
        ollama_endpoint: "http://127.0.0.1:11434".to_string(),
        ollama_model: "llama-3.2:3b".to_string(),
        deepl_api_key: "deepl-key-fx:fx".to_string(),
        openai_endpoint: "https://api.groq.com/openai/v1".to_string(),
        openai_api_key: "gsk_abc123".to_string(),
        openai_model: "llama-3.3-70b-versatile".to_string(),
        binarize: true,
        blur_subtraction: false,
        hud_position: "top".to_string(),
        hud_persistent: true,
        hud_offset_y: 60,
        font_size: 28,
        hud_duration_ms: 8000,
        hud_opacity: 85,
        settle_duration_ms: 350,
        glossary: std::collections::HashMap::from([
            ("Potion".to_string(), "İksir".to_string()),
            ("Sword".to_string(), "Kılıç".to_string()),
        ]),
    };

    // Serialize to RON
    let serialized = ron::to_string(&cfg).expect("Config must serialize to RON");
    assert!(!serialized.is_empty());

    // Deserialize back from RON
    let deserialized: Config =
        ron::from_str(&serialized).expect("Config must deserialize from RON");
    assert_eq!(cfg, deserialized);
}

#[test]
fn test_blur_subtraction_config_roundtrip() {
    let cfg = Config {
        blur_subtraction: true,
        ..Default::default()
    };

    let serialized = ron::to_string(&cfg).expect("Config with blur_subtraction must serialize");
    let deserialized: Config = ron::from_str(&serialized).expect("Config must deserialize");
    assert!(deserialized.blur_subtraction);
}

#[test]
fn test_ui_glossary_term_management() {
    let mut config = Config::default();
    let mut state = SettingsState::new();

    // 1. Initial empty glossary inputs
    assert!(state.glossary.new_source.is_empty());
    assert!(state.glossary.new_target.is_empty());

    // 2. Populate inputs
    state.glossary.new_source = "  Phoenix Down  ".to_string();
    state.glossary.new_target = "  Anka Kuşu Tüyü  ".to_string();

    let src = state.glossary.new_source.trim().to_string();
    let trg = state.glossary.new_target.trim().to_string();
    assert_eq!(src, "Phoenix Down");
    assert_eq!(trg, "Anka Kuşu Tüyü");

    // Add to config
    config.glossary.insert(src.clone(), trg.clone());
    assert_eq!(
        config.glossary.get("Phoenix Down"),
        Some(&"Anka Kuşu Tüyü".to_string())
    );

    // 3. Remove term
    config.glossary.remove(&src);
    assert!(!config.glossary.contains_key("Phoenix Down"));
}

#[test]
fn test_ui_history_view_and_search() {
    cosmic_applet_translator::i18n::init(&[]);
    let config = Config::default();
    let mut backlog = DialogueBacklog::new(50);
    backlog.push_with_timestamp(
        "14:20:10".into(),
        Some("Protagonist".into()),
        "Let us venture forth.".into(),
        "İleriye atılalım.".into(),
    );
    backlog.push_with_timestamp(
        "14:20:25".into(),
        None,
        "The wind howls in the distance.".into(),
        "Uzaklarda rüzgar uğulduyor.".into(),
    );

    let mut state = SettingsState::new();
    state.active_tab = SettingsTab::History;

    // View with all entries
    let _ = state.view(&config, &backlog);

    // View with search filter
    state.history.query = "protagonist".to_string();
    assert_eq!(backlog.search(&state.history.query).len(), 1);
    let _ = state.view(&config, &backlog);

    // View with non-matching filter
    state.history.query = "nonexistent".to_string();
    assert_eq!(backlog.search(&state.history.query).len(), 0);
    let _ = state.view(&config, &backlog);
}

#[test]
fn test_ui_ignore_roi_controls() {
    cosmic_applet_translator::i18n::init(&[]);
    let mut config = Config::default();
    let backlog = DialogueBacklog::default();
    let mut state = SettingsState::new();
    state.active_tab = SettingsTab::Capture;

    // 1. Initial without ignore_roi
    assert!(config.ignore_roi.is_none());
    let _ = state.view(&config, &backlog);

    // 2. With ignore_roi configured
    config.ignore_roi = Some(Roi::new(50, 100, 200, 300));
    assert!(config.ignore_roi.is_some());
    let _ = state.view(&config, &backlog);

    // 3. Clear ignore_roi
    config.ignore_roi = None;
    assert!(config.ignore_roi.is_none());
    let _ = state.view(&config, &backlog);
}

#[test]
fn test_i18n_fallback_and_language_selection() {
    // 1. Fallback initialization (en by default)
    cosmic_applet_translator::i18n::init(&[]);
    assert_eq!(cosmic_applet_translator::fl!("status"), "Status");
    assert_eq!(cosmic_applet_translator::fl!("active"), "Active");
    assert_eq!(cosmic_applet_translator::fl!("paused"), "Paused");
    assert_eq!(
        cosmic_applet_translator::fl!("tab-capture"),
        "Capture & Area"
    );
    assert_eq!(
        cosmic_applet_translator::fl!("tab-engine"),
        "Translation Engine"
    );
    assert_eq!(cosmic_applet_translator::fl!("tab-display"), "Subtitle HUD");
    assert_eq!(
        cosmic_applet_translator::fl!("tab-glossary"),
        "Game Glossary"
    );
    assert_eq!(
        cosmic_applet_translator::fl!("tab-history"),
        "Dialogue History"
    );
    assert_eq!(
        cosmic_applet_translator::fl!("ignore-roi"),
        "Ignore ROI (Bust/Portrait)"
    );
    assert_eq!(
        cosmic_applet_translator::fl!("blur-subtraction"),
        "High-Pass Blur Subtraction"
    );

    // 2. Select Turkish language
    let tr_lang: i18n_embed::unic_langid::LanguageIdentifier =
        "tr".parse().expect("Valid Turkish langid");
    cosmic_applet_translator::i18n::init(&[tr_lang]);
    assert_eq!(cosmic_applet_translator::fl!("status"), "Durum");
    assert_eq!(cosmic_applet_translator::fl!("active"), "Aktif");
    assert_eq!(cosmic_applet_translator::fl!("paused"), "Duraklatıldı");
    assert_eq!(
        cosmic_applet_translator::fl!("tab-capture"),
        "Yakalama & Alan"
    );
    assert_eq!(cosmic_applet_translator::fl!("tab-engine"), "Çeviri Motoru");
    assert_eq!(cosmic_applet_translator::fl!("tab-display"), "Altyazı HUD");
    assert_eq!(
        cosmic_applet_translator::fl!("tab-glossary"),
        "Oyun Sözlüğü"
    );
    assert_eq!(
        cosmic_applet_translator::fl!("tab-history"),
        "Diyalog Geçmişi"
    );
    assert_eq!(
        cosmic_applet_translator::fl!("ignore-roi"),
        "Hariç Tutulacak Alan (Portre/Bust)"
    );
    assert_eq!(
        cosmic_applet_translator::fl!("blur-subtraction"),
        "Yüksek Geçiren Bulanıklık Filtresi"
    );

    // 3. Fallback on unknown language (e.g. "xx-YY")
    let unknown_lang: i18n_embed::unic_langid::LanguageIdentifier =
        "xx-YY".parse().expect("Valid fallback langid");
    cosmic_applet_translator::i18n::init(&[unknown_lang]);
    // Should fallback cleanly without panic
    assert_eq!(cosmic_applet_translator::fl!("status"), "Status");
    assert_eq!(
        cosmic_applet_translator::fl!("tab-history"),
        "Dialogue History"
    );

    // 4. Reset to en
    let en_lang: i18n_embed::unic_langid::LanguageIdentifier =
        "en".parse().expect("Valid English langid");
    cosmic_applet_translator::i18n::init(&[en_lang]);
    assert_eq!(cosmic_applet_translator::fl!("status"), "Status");
}
