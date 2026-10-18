use crate::config::Config;
use cosmic::iced::Alignment;
use cosmic::widget::{self, button, column, container, row, text};
use cosmic::Element;
use translator_core::capture::roi::RoiPreset;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    Capture,
    Engine,
    Display,
    Glossary,
}

#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    pub active_tab: SettingsTab,
    pub ollama_test_status: Option<String>,
    pub is_testing_ollama: bool,
    pub new_glossary_source: String,
    pub new_glossary_target: String,
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    TabSelected(SettingsTab),
    EngineSelected(String),
    PresetSelected(RoiPreset),
    SelectCustomRoi,
    BinarizeToggled(bool),
    SettleDurationChanged(u64),
    OllamaEndpointChanged(String),
    OllamaModelChanged(String),
    DeepLApiKeyChanged(String),
    OpenAiEndpointChanged(String),
    OpenAiApiKeyChanged(String),
    OpenAiModelChanged(String),
    TargetLangChanged(String),
    SourceLangChanged(String),
    TestOllamaConnection,
    OllamaTestResult(Result<Vec<String>, String>),
    HudPositionChanged(String),
    HudPersistentToggled(bool),
    HudOffsetYChanged(u32),
    FontSizeChanged(u16),
    OpacityChanged(u8),
    DurationChanged(u64),
    TestHudRequested,
    GlossarySourceChanged(String),
    GlossaryTargetChanged(String),
    AddGlossaryTerm,
    RemoveGlossaryTerm(String),
    Close,
}

impl SettingsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn view<'a>(&'a self, config: &'a Config) -> Element<'a, SettingsMessage> {
        let nav_bar = row![
            tab_button("Yakalama & Alan", SettingsTab::Capture, self.active_tab),
            tab_button("Çeviri Motoru", SettingsTab::Engine, self.active_tab),
            tab_button("Altyazı HUD", SettingsTab::Display, self.active_tab),
            tab_button("Oyun Sözlüğü", SettingsTab::Glossary, self.active_tab),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let content: Element<'a, SettingsMessage> = match self.active_tab {
            SettingsTab::Capture => self.view_capture(config),
            SettingsTab::Engine => self.view_engine(config),
            SettingsTab::Display => self.view_display(config),
            SettingsTab::Glossary => self.view_glossary(config),
        };

        let footer = row![
            cosmic::iced::widget::space::horizontal(),
            button::standard("Pencereyi Kapat").on_press(SettingsMessage::Close),
        ]
        .width(cosmic::iced::Length::Fill);

        widget::container(
            column![
                nav_bar,
                widget::divider::horizontal::default(),
                widget::scrollable(content).height(cosmic::iced::Length::Fill),
                widget::divider::horizontal::default(),
                footer,
            ]
            .spacing(12)
            .padding(16)
        )
        .width(cosmic::iced::Length::Fill)
        .height(cosmic::iced::Length::Fill)
        .class(cosmic::theme::Container::WindowBackground)
        .into()
    }

    fn view_capture<'a>(&'a self, config: &'a Config) -> Element<'a, SettingsMessage> {
        let current_preset = RoiPreset::from_str_name(&config.preset);

        let mut preset_row = row![].spacing(6);
        for p in RoiPreset::ALL {
            let label = match p {
                RoiPreset::Bottom25 => "Alt %25 (RPG)",
                RoiPreset::Bottom33 => "Alt %33 (VN)",
                RoiPreset::FullScreen => "Tam Ekran",
                RoiPreset::Custom => "Özel Alan",
            };
            let is_active = p == current_preset;
            let btn = if is_active {
                button::suggested(label)
            } else {
                button::standard(label)
            };
            preset_row = preset_row.push(btn.on_press(SettingsMessage::PresetSelected(p)));
        }

        let custom_coords = if let Some(r) = config.custom_roi {
            format!("{}x{} @ ({}, {})", r.width, r.height, r.x, r.y)
        } else {
            "Tanımlı özel alan yok".to_string()
        };

        let section = widget::settings::section()
            .title("Diyalog ve Yakalama Alanı")
            .add(widget::settings::item(
                "Diyalog Şablonu",
                preset_row,
            ))
            .add(widget::settings::item(
                "Özel Koordinatlar",
                row![
                    text(custom_coords),
                    button::standard("Yeni Alan Çiz").on_press(SettingsMessage::SelectCustomRoi),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
            .add(widget::settings::item(
                "Otsu Binarizasyonu",
                widget::toggler(config.binarize).on_toggle(SettingsMessage::BinarizeToggled),
            ))
            .add(widget::settings::item(
                "Kare Sabitlenme Eşiği",
                row![
                    widget::slider(
                        100..=1000,
                        config.settle_duration_ms as u32,
                        |val| SettingsMessage::SettleDurationChanged(val as u64),
                    )
                    .width(180),
                    text(format!("{} ms", config.settle_duration_ms)).size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ));

        section.into()
    }

    fn view_engine<'a>(&'a self, config: &'a Config) -> Element<'a, SettingsMessage> {
        let active_engine = config.engine.as_str();
        let engine_buttons = row![
            if active_engine == "google" {
                button::suggested("Google")
            } else {
                button::standard("Google")
            }
            .on_press(SettingsMessage::EngineSelected("google".to_string())),
            if active_engine == "deepl" {
                button::suggested("DeepL")
            } else {
                button::standard("DeepL")
            }
            .on_press(SettingsMessage::EngineSelected("deepl".to_string())),
            if active_engine == "ollama" {
                button::suggested("Ollama")
            } else {
                button::standard("Ollama")
            }
            .on_press(SettingsMessage::EngineSelected("ollama".to_string())),
            if active_engine == "openai" {
                button::suggested("OpenAI/LLM")
            } else {
                button::standard("OpenAI/LLM")
            }
            .on_press(SettingsMessage::EngineSelected("openai".to_string())),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let mut section = widget::settings::section()
            .title("Çeviri Motoru Seçimi")
            .add(widget::settings::item("Aktif Motor", engine_buttons))
            .add(widget::settings::item(
                "Hedef Dil (Target)",
                widget::text_input("tr", &config.target_lang)
                    .on_input(SettingsMessage::TargetLangChanged)
                    .width(100),
            ))
            .add(widget::settings::item(
                "Kaynak Dil (Source)",
                widget::text_input("auto", &config.source_lang)
                    .on_input(SettingsMessage::SourceLangChanged)
                    .width(100),
            ));

        match active_engine {
            "google" => {
                section = section.add(widget::settings::item(
                    "Motor Bilgisi",
                    text("Google Translate devrede. Hızlıdır, anahtar veya model indirme gerektirmez.").size(13),
                ));
            }
            "deepl" => {
                section = section.add(widget::settings::item(
                    "DeepL API Key",
                    widget::text_input("DeepL API Key (:fx veya pro)", &config.deepl_api_key)
                        .on_input(SettingsMessage::DeepLApiKeyChanged)
                        .width(280),
                ));
            }
            "openai" => {
                section = section
                    .add(widget::settings::item(
                        "API Endpoint",
                        widget::text_input("https://api.openai.com/v1", &config.openai_endpoint)
                            .on_input(SettingsMessage::OpenAiEndpointChanged)
                            .width(280),
                    ))
                    .add(widget::settings::item(
                        "Model Adı",
                        widget::text_input("gpt-4o-mini", &config.openai_model)
                            .on_input(SettingsMessage::OpenAiModelChanged)
                            .width(280),
                    ))
                    .add(widget::settings::item(
                        "API Key",
                        widget::text_input("sk-...", &config.openai_api_key)
                            .on_input(SettingsMessage::OpenAiApiKeyChanged)
                            .width(280),
                    ));
            }
            _ => {
                let status_desc = if self.is_testing_ollama {
                    "Bağlantı test ediliyor...".to_string()
                } else if let Some(ref st) = self.ollama_test_status {
                    st.clone()
                } else {
                    "Durum bilinmiyor".to_string()
                };

                section = section
                    .add(widget::settings::item(
                        "Ollama Endpoint",
                        widget::text_input("http://localhost:11434", &config.ollama_endpoint)
                            .on_input(SettingsMessage::OllamaEndpointChanged)
                            .width(260),
                    ))
                    .add(widget::settings::item(
                        "Ollama Modeli",
                        widget::text_input("qwen2.5:3b", &config.ollama_model)
                            .on_input(SettingsMessage::OllamaModelChanged)
                            .width(260),
                    ))
                    .add(widget::settings::item(
                        "Bağlantı Testi",
                        row![
                            text(status_desc).size(13),
                            button::suggested("Test Et")
                                .on_press(SettingsMessage::TestOllamaConnection),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                    ));
            }
        }

        section.into()
    }

    fn view_display<'a>(&'a self, config: &'a Config) -> Element<'a, SettingsMessage> {
        let current_pos = config.hud_position.as_str();
        let position_row = row![
            if current_pos == "bottom" || (current_pos != "top" && current_pos != "center") {
                button::suggested("Ekran Altı")
            } else {
                button::standard("Ekran Altı")
            }
            .on_press(SettingsMessage::HudPositionChanged("bottom".to_string())),
            if current_pos == "top" {
                button::suggested("Ekran Üstü")
            } else {
                button::standard("Ekran Üstü")
            }
            .on_press(SettingsMessage::HudPositionChanged("top".to_string())),
            if current_pos == "center" {
                button::suggested("Ekran Ortası")
            } else {
                button::standard("Ekran Ortası")
            }
            .on_press(SettingsMessage::HudPositionChanged("center".to_string())),
        ]
        .spacing(6);

        let mut section = widget::settings::section()
            .title("Altyazı HUD Katmanı")
            .add(widget::settings::item("Altyazı Konumu", position_row))
            .add(widget::settings::item(
                "Dikey Mesafe (Ofset)",
                row![
                    widget::slider(0..=300, config.hud_offset_y, SettingsMessage::HudOffsetYChanged)
                        .width(180),
                    text(format!("{} px", config.hud_offset_y)).size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
            .add(widget::settings::item(
                "Font Boyutu",
                row![
                    widget::slider(16..=36, config.font_size, SettingsMessage::FontSizeChanged)
                        .width(180),
                    text(format!("{} px", config.font_size)).size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
            .add(widget::settings::item(
                "Arka Plan Opaklığı",
                row![
                    widget::slider(20..=100, config.hud_opacity, SettingsMessage::OpacityChanged)
                        .width(180),
                    text(format!("{} %", config.hud_opacity)).size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
            .add(widget::settings::item(
                "Altyazıyı Ekranda Tut (Sürekli)",
                widget::toggler(config.hud_persistent).on_toggle(SettingsMessage::HudPersistentToggled),
            ));

        if !config.hud_persistent {
            let dur = if config.hud_duration_ms == 0 { 6000 } else { config.hud_duration_ms };
            section = section.add(widget::settings::item(
                "Otomatik Kaybolma Süresi",
                row![
                    widget::slider(
                        2000..=15000,
                        dur as u32,
                        |val| SettingsMessage::DurationChanged(val as u64),
                    )
                    .width(180),
                    text(format!("{:.1} sn", dur as f32 / 1000.0)).size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ));
        }

        section = section.add(widget::settings::item(
            "Canlı Önizleme",
            button::standard("HUD'da Test Mesajı Göster")
                .on_press(SettingsMessage::TestHudRequested),
        ));

        section.into()
    }

    fn view_glossary<'a>(&'a self, config: &'a Config) -> Element<'a, SettingsMessage> {
        let add_row = row![
            widget::text_input("Orijinal Terim (örn: Potion)", &self.new_glossary_source)
                .on_input(SettingsMessage::GlossarySourceChanged)
                .width(180),
            widget::text_input("Çevirisi (örn: İksir)", &self.new_glossary_target)
                .on_input(SettingsMessage::GlossaryTargetChanged)
                .width(180),
            button::suggested("Ekle").on_press(SettingsMessage::AddGlossaryTerm),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut list_col = column![].spacing(8);
        for (term, trans) in &config.glossary {
            let item_row = row![
                text(format!("\"{}\" ➔ \"{}\"", term, trans)).size(14),
                cosmic::iced::widget::space::horizontal(),
                button::destructive("Sil")
                    .on_press(SettingsMessage::RemoveGlossaryTerm(term.clone())),
            ]
            .width(cosmic::iced::Length::Fill)
            .align_y(Alignment::Center);

            list_col = list_col.push(container(item_row).padding(6));
        }

        let section = widget::settings::section()
            .title("Oyun Sözlüğü (Glossary)")
            .add(widget::settings::item(
                "Yeni Terim Ekle",
                add_row,
            ));

        column![
            section,
            text("Kayıtlı Terimler:").size(14),
            if config.glossary.is_empty() {
                Element::from(text("Henüz kayıtlı sözlük terimi bulunmuyor.").size(13))
            } else {
                Element::from(list_col)
            }
        ]
        .spacing(12)
        .into()
    }
}

fn tab_button<'a>(
    title: &'static str,
    tab: SettingsTab,
    active: SettingsTab,
) -> Element<'a, SettingsMessage> {
    if tab == active {
        button::suggested(title)
            .on_press(SettingsMessage::TabSelected(tab))
            .into()
    } else {
        button::standard(title)
            .on_press(SettingsMessage::TabSelected(tab))
            .into()
    }
}
