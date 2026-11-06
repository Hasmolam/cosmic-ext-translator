use cosmic_applet_translator::config::Config;
use cosmic_applet_translator::history::DialogueBacklog;
use cosmic_applet_translator::settings_dialog::{SettingsState, SettingsTab};
use translator_core::capture::roi::{Roi, RoiPreset};

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
    let deserialized: Config = ron::from_str(&serialized).expect("Config must deserialize from RON");
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
    assert!(state.new_glossary_source.is_empty());
    assert!(state.new_glossary_target.is_empty());

    // 2. Populate inputs
    state.new_glossary_source = "  Phoenix Down  ".to_string();
    state.new_glossary_target = "  Anka Kuşu Tüyü  ".to_string();

    let src = state.new_glossary_source.trim().to_string();
    let trg = state.new_glossary_target.trim().to_string();
    assert_eq!(src, "Phoenix Down");
    assert_eq!(trg, "Anka Kuşu Tüyü");

    // Add to config
    config.glossary.insert(src.clone(), trg.clone());
    assert_eq!(config.glossary.get("Phoenix Down"), Some(&"Anka Kuşu Tüyü".to_string()));

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
    state.history_search_query = "protagonist".to_string();
    assert_eq!(backlog.search(&state.history_search_query).len(), 1);
    let _ = state.view(&config, &backlog);

    // View with non-matching filter
    state.history_search_query = "nonexistent".to_string();
    assert_eq!(backlog.search(&state.history_search_query).len(), 0);
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
    assert_eq!(cosmic_applet_translator::fl!("tab-capture"), "Capture & Area");
    assert_eq!(cosmic_applet_translator::fl!("tab-engine"), "Translation Engine");
    assert_eq!(cosmic_applet_translator::fl!("tab-display"), "Subtitle HUD");
    assert_eq!(cosmic_applet_translator::fl!("tab-glossary"), "Game Glossary");
    assert_eq!(cosmic_applet_translator::fl!("tab-history"), "Dialogue History");
    assert_eq!(cosmic_applet_translator::fl!("ignore-roi"), "Ignore ROI (Bust/Portrait)");
    assert_eq!(cosmic_applet_translator::fl!("blur-subtraction"), "High-Pass Blur Subtraction");

    // 2. Select Turkish language
    let tr_lang: i18n_embed::unic_langid::LanguageIdentifier = "tr".parse().expect("Valid Turkish langid");
    cosmic_applet_translator::i18n::init(&[tr_lang]);
    assert_eq!(cosmic_applet_translator::fl!("status"), "Durum");
    assert_eq!(cosmic_applet_translator::fl!("active"), "Aktif");
    assert_eq!(cosmic_applet_translator::fl!("paused"), "Duraklatıldı");
    assert_eq!(cosmic_applet_translator::fl!("tab-capture"), "Yakalama & Alan");
    assert_eq!(cosmic_applet_translator::fl!("tab-engine"), "Çeviri Motoru");
    assert_eq!(cosmic_applet_translator::fl!("tab-display"), "Altyazı HUD");
    assert_eq!(cosmic_applet_translator::fl!("tab-glossary"), "Oyun Sözlüğü");
    assert_eq!(cosmic_applet_translator::fl!("tab-history"), "Diyalog Geçmişi");
    assert_eq!(cosmic_applet_translator::fl!("ignore-roi"), "Hariç Tutulacak Alan (Portre/Bust)");
    assert_eq!(cosmic_applet_translator::fl!("blur-subtraction"), "Yüksek Geçiren Bulanıklık Filtresi");

    // 3. Fallback on unknown language (e.g. "xx-YY")
    let unknown_lang: i18n_embed::unic_langid::LanguageIdentifier = "xx-YY".parse().expect("Valid fallback langid");
    cosmic_applet_translator::i18n::init(&[unknown_lang]);
    // Should fallback cleanly without panic
    assert_eq!(cosmic_applet_translator::fl!("status"), "Status");
    assert_eq!(cosmic_applet_translator::fl!("tab-history"), "Dialogue History");

    // 4. Reset to en
    let en_lang: i18n_embed::unic_langid::LanguageIdentifier = "en".parse().expect("Valid English langid");
    cosmic_applet_translator::i18n::init(&[en_lang]);
    assert_eq!(cosmic_applet_translator::fl!("status"), "Status");
}

#[test]
fn test_overlay_stdout_communication_protocol() {
    #[derive(serde::Deserialize)]
    struct SelectedPayload {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        surface_width: Option<u32>,
        surface_height: Option<u32>,
    }

    // 1. Simulate exact output emitted by cosmic-translator-overlay writeln!
    let simulated_roi = Roi::new(150, 250, 800, 450);
    let surface_w = 1920;
    let surface_h = 1080;
    let mut stdout_buf = Vec::new();
    use std::io::Write;
    let _ = writeln!(
        &mut stdout_buf,
        "ROI_SELECTED:{{\"x\":{},\"y\":{},\"width\":{},\"height\":{},\"surface_width\":{},\"surface_height\":{}}}",
        simulated_roi.x, simulated_roi.y, simulated_roi.width, simulated_roi.height, surface_w, surface_h
    );

    let stdout_str = String::from_utf8_lossy(&stdout_buf);
    let mut parsed_roi = None;
    let mut parsed_sw = None;
    let mut parsed_sh = None;
    for line in stdout_str.lines() {
        if let Some(json_str) = line.strip_prefix("ROI_SELECTED:") {
            if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str) {
                parsed_roi = Some(Roi::new(p.x, p.y, p.width, p.height));
                parsed_sw = p.surface_width;
                parsed_sh = p.surface_height;
            }
        }
    }

    assert_eq!(parsed_roi, Some(simulated_roi));
    assert_eq!(parsed_sw, Some(surface_w));
    assert_eq!(parsed_sh, Some(surface_h));

    // 2. Cancellation output
    let mut cancel_buf = Vec::new();
    let _ = writeln!(&mut cancel_buf, "ROI_CANCELLED");
    let cancel_str = String::from_utf8_lossy(&cancel_buf);
    let is_cancelled = cancel_str.lines().any(|l| l.trim() == "ROI_CANCELLED");
    assert!(is_cancelled);

    // 3. Multi-line noise with trailing output
    let noisy_output = "INFO: starting layer surface\nDEBUG: attached to output\nROI_SELECTED:{\"x\":10,\"y\":20,\"width\":30,\"height\":40,\"surface_width\":1000,\"surface_height\":800}\nINFO: layer surface destroyed\n";
    let mut found = None;
    for line in noisy_output.lines() {
        if let Some(json_str) = line.strip_prefix("ROI_SELECTED:") {
            if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str) {
                found = Some(Roi::new(p.x, p.y, p.width, p.height));
            }
        }
    }
    assert_eq!(found, Some(Roi::new(10, 20, 30, 40)));

    // 4. Missing optional surface dimensions
    let no_surface_json = "ROI_SELECTED:{\"x\":5,\"y\":15,\"width\":25,\"height\":35}";
    let mut no_surface_roi = None;
    let mut no_surface_w = None;
    if let Some(json_str) = no_surface_json.strip_prefix("ROI_SELECTED:") {
        if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str) {
            no_surface_roi = Some(Roi::new(p.x, p.y, p.width, p.height));
            no_surface_w = p.surface_width;
        }
    }
    assert_eq!(no_surface_roi, Some(Roi::new(5, 15, 25, 35)));
    assert_eq!(no_surface_w, None);

    // 5. Malformed json payload handled gracefully without panic
    let malformed_cases = [
        "ROI_SELECTED:",
        "ROI_SELECTED:{}",
        "ROI_SELECTED:{\"x\":\"not_a_number\"}",
        "ROI_SELECTED:invalid_json_data",
        "ROI_SELECTED:{\"x\":-1,\"y\":0,\"width\":10,\"height\":10}",
    ];
    for malformed in malformed_cases {
        let mut parsed = false;
        if let Some(json_str) = malformed.strip_prefix("ROI_SELECTED:") {
            if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str) {
                let _ = Roi::new(p.x, p.y, p.width, p.height);
                parsed = true;
            }
        }
        assert!(!parsed, "Malformed case '{malformed}' should not parse successfully");
    }
}



