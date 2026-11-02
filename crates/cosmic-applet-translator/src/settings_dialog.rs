use crate::config::Config;
use crate::fl;
use crate::history::DialogueBacklog;
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
    History,
}

#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    pub active_tab: SettingsTab,
    pub ollama_test_status: Option<String>,
    pub is_testing_ollama: bool,
    pub new_glossary_source: String,
    pub new_glossary_target: String,
    pub history_search_query: String,
}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    TabSelected(SettingsTab),
    EngineSelected(String),
    PresetSelected(RoiPreset),
    SelectCustomRoi,
    SelectIgnoreRoi,
    ClearIgnoreRoi,
    BinarizeToggled(bool),
    BlurSubtractionToggled(bool),
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
    StartMortPositioning,
    GlossarySourceChanged(String),
    GlossaryTargetChanged(String),
    AddGlossaryTerm,
    RemoveGlossaryTerm(String),
    HistorySearchChanged(String),
    ClearHistory,
    CopyHistoryEntry(String),
    Close,
}

impl SettingsState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn view<'a>(
        &'a self,
        config: &'a Config,
        backlog: &'a DialogueBacklog,
    ) -> Element<'a, SettingsMessage> {
        let nav_bar = row![
            tab_button(fl!("tab-capture"), SettingsTab::Capture, self.active_tab),
            tab_button(fl!("tab-engine"), SettingsTab::Engine, self.active_tab),
            tab_button(fl!("tab-display"), SettingsTab::Display, self.active_tab),
            tab_button(fl!("tab-glossary"), SettingsTab::Glossary, self.active_tab),
            tab_button(fl!("tab-history"), SettingsTab::History, self.active_tab),
        ]
        .spacing(6)
        .align_y(Alignment::Center);

        let content: Element<'a, SettingsMessage> = match self.active_tab {
            SettingsTab::Capture => self.view_capture(config),
            SettingsTab::Engine => self.view_engine(config),
            SettingsTab::Display => self.view_display(config),
            SettingsTab::Glossary => self.view_glossary(config),
            SettingsTab::History => self.view_history(config, backlog),
        };

        let footer = row![
            cosmic::iced::widget::space::horizontal(),
            button::standard(fl!("close-window")).on_press(SettingsMessage::Close),
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
            .spacing(12),
        )
        .width(cosmic::iced::Length::Fill)
        .height(cosmic::iced::Length::Fill)
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
            fl!("no-custom-area")
        };

        let ignore_coords = if let Some(r) = config.ignore_roi {
            format!("{}x{} @ ({}, {})", r.width, r.height, r.x, r.y)
        } else {
            fl!("no-ignore-area")
        };

        let mut ignore_row = row![
            text(ignore_coords),
            button::standard(fl!("draw-ignore-area")).on_press(SettingsMessage::SelectIgnoreRoi),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        if config.ignore_roi.is_some() {
            ignore_row = ignore_row.push(
                button::destructive(fl!("clear-ignore-area")).on_press(SettingsMessage::ClearIgnoreRoi),
            );
        }

        let section = widget::settings::section()
            .title(fl!("section-capture-title"))
            .add(widget::settings::item(
                fl!("preset-dialog-template"),
                preset_row,
            ))
            .add(widget::settings::item(
                fl!("custom-coords"),
                row![
                    text(custom_coords),
                    button::standard(fl!("draw-new-area")).on_press(SettingsMessage::SelectCustomRoi),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
            .add(widget::settings::item(
                fl!("ignore-roi"),
                ignore_row,
            ))
            .add(widget::settings::item(
                fl!("otsu-binarization"),
                widget::toggler(config.binarize).on_toggle(SettingsMessage::BinarizeToggled),
            ))
            .add(widget::settings::item(
                fl!("blur-subtraction"),
                widget::toggler(config.blur_subtraction).on_toggle(SettingsMessage::BlurSubtractionToggled),
            ))
            .add(widget::settings::item(
                fl!("settle-threshold"),
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
            .title(fl!("engine-selection"))
            .add(widget::settings::item(fl!("active-engine"), engine_buttons))
            .add(widget::settings::item(
                fl!("target-language"),
                widget::text_input("tr", &config.target_lang)
                    .on_input(SettingsMessage::TargetLangChanged)
                    .width(100),
            ))
            .add(widget::settings::item(
                fl!("source-language"),
                widget::text_input("auto", &config.source_lang)
                    .on_input(SettingsMessage::SourceLangChanged)
                    .width(100),
            ));

        match active_engine {
            "google" => {
                section = section.add(widget::settings::item(
                    fl!("engine-info"),
                    text(fl!("engine-info-google")).size(13),
                ));
            }
            "deepl" => {
                section = section.add(widget::settings::item(
                    fl!("deepl-api-key"),
                    widget::text_input("DeepL API Key (:fx veya pro)", &config.deepl_api_key)
                        .on_input(SettingsMessage::DeepLApiKeyChanged)
                        .width(280),
                ));
            }
            "openai" => {
                section = section
                    .add(widget::settings::item(
                        fl!("api-endpoint"),
                        widget::text_input("https://api.openai.com/v1", &config.openai_endpoint)
                            .on_input(SettingsMessage::OpenAiEndpointChanged)
                            .width(280),
                    ))
                    .add(widget::settings::item(
                        fl!("model-name"),
                        widget::text_input("gpt-4o-mini", &config.openai_model)
                            .on_input(SettingsMessage::OpenAiModelChanged)
                            .width(280),
                    ))
                    .add(widget::settings::item(
                        fl!("api-key"),
                        widget::text_input("sk-...", &config.openai_api_key)
                            .on_input(SettingsMessage::OpenAiApiKeyChanged)
                            .width(280),
                    ));
            }
            _ => {
                let status_desc = if self.is_testing_ollama {
                    fl!("testing-connection")
                } else if let Some(ref st) = self.ollama_test_status {
                    st.clone()
                } else {
                    fl!("status-unknown")
                };

                section = section
                    .add(widget::settings::item(
                        fl!("ollama-endpoint"),
                        widget::text_input("http://localhost:11434", &config.ollama_endpoint)
                            .on_input(SettingsMessage::OllamaEndpointChanged)
                            .width(260),
                    ))
                    .add(widget::settings::item(
                        fl!("ollama-model"),
                        widget::text_input("qwen2.5:3b", &config.ollama_model)
                            .on_input(SettingsMessage::OllamaModelChanged)
                            .width(260),
                    ))
                    .add(widget::settings::item(
                        fl!("connection-test"),
                        row![
                            text(status_desc).size(13),
                            button::suggested(fl!("test-button"))
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
                button::suggested(fl!("pos-bottom"))
            } else {
                button::standard(fl!("pos-bottom"))
            }
            .on_press(SettingsMessage::HudPositionChanged("bottom".to_string())),
            if current_pos == "top" {
                button::suggested(fl!("pos-top"))
            } else {
                button::standard(fl!("pos-top"))
            }
            .on_press(SettingsMessage::HudPositionChanged("top".to_string())),
            if current_pos == "center" {
                button::suggested(fl!("pos-center"))
            } else {
                button::standard(fl!("pos-center"))
            }
            .on_press(SettingsMessage::HudPositionChanged("center".to_string())),
        ]
        .spacing(6);

        let mut section = widget::settings::section()
            .title(fl!("hud-layer-title"))
            .add(widget::settings::item(fl!("hud-position"), position_row))
            .add(widget::settings::item(
                fl!("hud-vertical-offset"),
                row![
                    widget::slider(0..=300, config.hud_offset_y, SettingsMessage::HudOffsetYChanged)
                        .width(180),
                    text(format!("{} px", config.hud_offset_y)).size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
            .add(widget::settings::item(
                fl!("font-size"),
                row![
                    widget::slider(16..=36, config.font_size, SettingsMessage::FontSizeChanged)
                        .width(180),
                    text(format!("{} px", config.font_size)).size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
            .add(widget::settings::item(
                fl!("background-opacity"),
                row![
                    widget::slider(20..=100, config.hud_opacity, SettingsMessage::OpacityChanged)
                        .width(180),
                    text(format!("{} %", config.hud_opacity)).size(14),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ))
            .add(widget::settings::item(
                fl!("hud-persistent"),
                widget::toggler(config.hud_persistent).on_toggle(SettingsMessage::HudPersistentToggled),
            ));

        if !config.hud_persistent {
            let dur = if config.hud_duration_ms == 0 { 6000 } else { config.hud_duration_ms };
            section = section.add(widget::settings::item(
                fl!("hud-auto-hide-time"),
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
            fl!("mort-positioning"),
            button::suggested(fl!("mort-button"))
                .on_press(SettingsMessage::StartMortPositioning),
        ));

        section = section.add(widget::settings::item(
            fl!("live-preview"),
            button::standard(fl!("show-test-hud-message"))
                .on_press(SettingsMessage::TestHudRequested),
        ));

        section.into()
    }

    fn view_glossary<'a>(&'a self, config: &'a Config) -> Element<'a, SettingsMessage> {
        let add_row = row![
            widget::text_input(fl!("glossary-source-placeholder"), &self.new_glossary_source)
                .on_input(SettingsMessage::GlossarySourceChanged)
                .width(180),
            widget::text_input(fl!("glossary-target-placeholder"), &self.new_glossary_target)
                .on_input(SettingsMessage::GlossaryTargetChanged)
                .width(180),
            button::suggested(fl!("add-button")).on_press(SettingsMessage::AddGlossaryTerm),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut list_col = column![].spacing(8);
        for (term, trans) in &config.glossary {
            let item_row = row![
                text(format!("\"{}\" ➔ \"{}\"", term, trans)).size(14),
                cosmic::iced::widget::space::horizontal(),
                button::destructive(fl!("delete-button"))
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

    fn view_history<'a>(
        &'a self,
        _config: &'a Config,
        backlog: &'a DialogueBacklog,
    ) -> Element<'a, SettingsMessage> {
        let search_bar = row![
            widget::text_input(
                fl!("history-search-placeholder"),
                &self.history_search_query
            )
            .on_input(SettingsMessage::HistorySearchChanged)
            .width(cosmic::iced::Length::Fill),
            button::destructive(fl!("clear-history")).on_press(SettingsMessage::ClearHistory),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let filtered_entries = backlog.search(&self.history_search_query);

        let entries_list: Element<'a, SettingsMessage> = if filtered_entries.is_empty() {
            widget::container(text(fl!("no-history-entries")).size(14))
                .padding(24)
                .center_x(cosmic::iced::Length::Fill)
                .into()
        } else {
            let mut col = column![].spacing(8);
            for entry in filtered_entries {
                let timestamp_txt = text(&entry.timestamp_str).size(12);

                let header = if let Some(ref speaker) = entry.speaker {
                    let speaker_badge = widget::container(text(speaker).size(12))
                        .padding(cosmic::iced::Padding::from([2, 8]))
                        .style(|_theme| widget::container::Style {
                            background: Some(cosmic::iced::Background::Color(cosmic::iced::Color::from_rgba(0.2, 0.5, 0.8, 0.25))),
                            border: cosmic::iced::Border {
                                color: cosmic::iced::Color::from_rgba(0.3, 0.6, 0.9, 0.5),
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..Default::default()
                        });
                    let copy_payload = format!("{}: {}\n{}", speaker, entry.source, entry.target);
                    row![
                        timestamp_txt,
                        speaker_badge,
                        cosmic::iced::widget::space::horizontal(),
                        button::standard(fl!("copy"))
                            .on_press(SettingsMessage::CopyHistoryEntry(copy_payload)),
                    ]
                } else {
                    let copy_payload = format!("{}\n{}", entry.source, entry.target);
                    row![
                        timestamp_txt,
                        cosmic::iced::widget::space::horizontal(),
                        button::standard(fl!("copy"))
                            .on_press(SettingsMessage::CopyHistoryEntry(copy_payload)),
                    ]
                }
                .spacing(8)
                .align_y(Alignment::Center);

                let source_txt = text(&entry.source)
                    .size(13)
                    .wrapping(cosmic::iced::widget::text::Wrapping::Word);

                let target_txt = text(&entry.target)
                    .size(14)
                    .wrapping(cosmic::iced::widget::text::Wrapping::Word);

                let card_content = column![
                    header,
                    widget::divider::horizontal::default(),
                    source_txt,
                    target_txt,
                ]
                .spacing(6);

                let card = widget::container(card_content)
                    .padding(10)
                    .width(cosmic::iced::Length::Fill)
                    .class(cosmic::theme::Container::Card);

                col = col.push(card);
            }
            col.into()
        };

        column![
            search_bar,
            widget::divider::horizontal::default(),
            entries_list,
        ]
        .spacing(12)
        .width(cosmic::iced::Length::Fill)
        .into()
    }
}

fn tab_button<'a>(
    title: impl Into<std::borrow::Cow<'a, str>>,
    tab: SettingsTab,
    active: SettingsTab,
) -> Element<'a, SettingsMessage> {
    let title_str = title.into();
    if tab == active {
        button::suggested(title_str)
            .on_press(SettingsMessage::TabSelected(tab))
            .into()
    } else {
        button::standard(title_str)
            .on_press(SettingsMessage::TabSelected(tab))
            .into()
    }
}
