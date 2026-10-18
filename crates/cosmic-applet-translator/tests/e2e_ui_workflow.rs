use cosmic_applet_translator::config::Config;
use cosmic_applet_translator::settings_dialog::{SettingsState, SettingsTab};
use translator_core::capture::roi::{Roi, RoiPreset};

#[test]
fn test_ui_settings_tab_transitions() {
    let config = Config::default();
    let mut state = SettingsState::new();

    // Verify initial tab is Capture
    assert_eq!(state.active_tab, SettingsTab::Capture);
    let _ = state.view(&config);

    // Switch to Engine tab
    state.active_tab = SettingsTab::Engine;
    assert_eq!(state.active_tab, SettingsTab::Engine);
    let _ = state.view(&config);

    // Switch to Display tab
    state.active_tab = SettingsTab::Display;
    assert_eq!(state.active_tab, SettingsTab::Display);
    let _ = state.view(&config);

    // Switch to Glossary tab
    state.active_tab = SettingsTab::Glossary;
    assert_eq!(state.active_tab, SettingsTab::Glossary);
    let _ = state.view(&config);
}

#[test]
fn test_ui_engine_views_render_cleanly() {
    let mut config = Config::default();
    let mut state = SettingsState::new();
    state.active_tab = SettingsTab::Engine;

    // 1. Google
    config.engine = "google".to_string();
    let _ = state.view(&config);

    // 2. DeepL
    config.engine = "deepl".to_string();
    config.deepl_api_key = "test-auth-key:fx".to_string();
    let _ = state.view(&config);

    // 3. Ollama
    config.engine = "ollama".to_string();
    config.ollama_endpoint = "http://localhost:11434".to_string();
    config.ollama_model = "qwen2.5:3b".to_string();
    let _ = state.view(&config);

    // 4. OpenAI
    config.engine = "openai".to_string();
    config.openai_endpoint = "https://api.openai.com/v1".to_string();
    config.openai_api_key = "sk-test-secret-key".to_string();
    config.openai_model = "gpt-4o-mini".to_string();
    let _ = state.view(&config);
}

#[test]
fn test_ui_config_persistence_and_ron_roundtrip() {
    let cfg = Config {
        enabled: true,
        target_lang: "tr".to_string(),
        source_lang: "ja".to_string(),
        restore_token: Some("test-restore-token-abc".to_string()),
        preset: RoiPreset::Custom.as_str().to_string(),
        custom_roi: Some(Roi::new(120, 240, 600, 300)),
        engine: "deepl".to_string(),
        ollama_endpoint: "http://127.0.0.1:11434".to_string(),
        ollama_model: "llama-3.2:3b".to_string(),
        deepl_api_key: "deepl-key-fx:fx".to_string(),
        openai_endpoint: "https://api.groq.com/openai/v1".to_string(),
        openai_api_key: "gsk_abc123".to_string(),
        openai_model: "llama-3.3-70b-versatile".to_string(),
        binarize: true,
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
