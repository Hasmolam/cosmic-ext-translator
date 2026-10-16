use std::sync::Arc;
use crate::config::Config;
use crate::fl;
use crate::settings_dialog::{SettingsMessage, SettingsState};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{window::Id, Limits, Task};
use cosmic::prelude::*;
use cosmic::widget::{self, button, row};
use tracing::{error, info};
use translator_core::capture::roi::{CaptureRegion, RoiPreset};
use translator_core::ipc::{HudIpcClient, HudMessage};
use translator_core::ocr::MockOcrBackend;
use translator_core::orchestrator::{
    spawn_orchestrator, OrchestratorConfig, OrchestratorHandle,
};
use translator_core::pipeline::debouncer::FrameDebouncer;
use translator_core::translation::OllamaTranslationBackend;
use translator_core::{CaptureSession, Roi};

#[derive(Default)]
pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    settings_window: Option<Id>,
    config: Config,
    config_handler: Option<cosmic_config::Config>,
    settings_state: SettingsState,
    capture_active: bool,
    orchestrator: Option<OrchestratorHandle>,
    capture_session: Option<CaptureSession>,
}

impl AppModel {
    fn save_config(&self) {
        if let Some(ref handler) = self.config_handler {
            let _ = self.config.write_entry(handler);
        }
    }
}

#[derive(Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    OpenSettings,
    CloseSettingsWindow(Id),
    Settings(SettingsMessage),
    ToggleCapture(bool),
    SelectRoiRequested,
    RoiSelected(Roi),
    RoiSelectionCancelled,
    PresetSelected(RoiPreset),
    CaptureStarted(CaptureSession),
    CaptureFailed(String),
    HudCleared,
}

impl Clone for Message {
    fn clone(&self) -> Self {
        match self {
            Self::TogglePopup => Self::TogglePopup,
            Self::PopupClosed(id) => Self::PopupClosed(*id),
            Self::OpenSettings => Self::OpenSettings,
            Self::CloseSettingsWindow(id) => Self::CloseSettingsWindow(*id),
            Self::Settings(m) => Self::Settings(m.clone()),
            Self::ToggleCapture(v) => Self::ToggleCapture(*v),
            Self::SelectRoiRequested => Self::SelectRoiRequested,
            Self::RoiSelected(r) => Self::RoiSelected(*r),
            Self::RoiSelectionCancelled => Self::RoiSelectionCancelled,
            Self::PresetSelected(p) => Self::PresetSelected(*p),
            Self::CaptureStarted(_) => Self::CaptureFailed("Cannot clone active session".into()),
            Self::CaptureFailed(s) => Self::CaptureFailed(s.clone()),
            Self::HudCleared => Self::HudCleared,
        }
    }
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.system76.CosmicExtLingua";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let (config_handler, config) = match cosmic_config::Config::new(Self::APP_ID, Config::VERSION) {
            Ok(context) => {
                let entry = match Config::get_entry(&context) {
                    Ok(cfg) => cfg,
                    Err((_errors, cfg)) => cfg,
                };
                (Some(context), entry)
            }
            Err(_) => (None, Config::default()),
        };

        let app = AppModel {
            core,
            popup: None,
            settings_window: None,
            config,
            config_handler,
            settings_state: SettingsState::new(),
            capture_active: false,
            orchestrator: None,
            capture_session: None,
        };

        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        if Some(id) == self.popup {
            Some(Message::PopupClosed(id))
        } else if Some(id) == self.settings_window {
            Some(Message::CloseSettingsWindow(id))
        } else {
            None
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(400.0)
                        .min_width(320.0)
                        .min_height(240.0)
                        .max_height(1080.0);
                    get_popup(popup_settings)
                };
            }

            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }

            Message::OpenSettings => {
                if self.settings_window.is_some() {
                    return Task::none();
                }
                let (id, spawn_task) = cosmic::iced::window::open(cosmic::iced::window::Settings {
                    size: (700.0, 520.0).into(),
                    min_size: Some((520.0, 380.0).into()),
                    resizable: true,
                    decorations: true,
                    ..Default::default()
                });
                self.settings_window = Some(id);
                let _ = self.set_window_title("cosmic-ext-lingua: Ayarlar".to_string(), id);
                return spawn_task.map(|_| cosmic::Action::None);
            }

            Message::CloseSettingsWindow(id) => {
                if self.settings_window == Some(id) {
                    self.settings_window = None;
                }
                return cosmic::iced::window::close(id);
            }

            Message::Settings(settings_msg) => {
                match settings_msg {
                    SettingsMessage::TabSelected(tab) => {
                        self.settings_state.active_tab = tab;
                    }
                    SettingsMessage::PresetSelected(preset) => {
                        self.config.preset = preset.as_str().to_string();
                        self.save_config();
                        if let Some(ref session) = self.capture_session {
                            let region = CaptureRegion::new(preset, self.config.custom_roi);
                            session.update_region(region);
                        }
                    }
                    SettingsMessage::SelectCustomRoi => {
                        return self.update(Message::SelectRoiRequested);
                    }
                    SettingsMessage::SettleDurationChanged(ms) => {
                        self.config.settle_duration_ms = ms;
                        self.save_config();
                    }
                    SettingsMessage::OllamaEndpointChanged(ep) => {
                        self.config.ollama_endpoint = ep;
                        self.save_config();
                    }
                    SettingsMessage::OllamaModelChanged(model) => {
                        self.config.ollama_model = model;
                        self.save_config();
                    }
                    SettingsMessage::TargetLangChanged(lang) => {
                        self.config.target_lang = lang;
                        self.save_config();
                    }
                    SettingsMessage::SourceLangChanged(lang) => {
                        self.config.source_lang = lang;
                        self.save_config();
                    }
                    SettingsMessage::TestOllamaConnection => {
                        self.settings_state.is_testing_ollama = true;
                        let endpoint = self.config.ollama_endpoint.clone();
                        let model = self.config.ollama_model.clone();
                        return Task::perform(
                            async move {
                                let backend = OllamaTranslationBackend::new(endpoint, model);
                                backend.test_connection().await.map_err(|e| e.to_string())
                            },
                            |res| cosmic::Action::from(Message::Settings(SettingsMessage::OllamaTestResult(res))),
                        );
                    }
                    SettingsMessage::OllamaTestResult(res) => {
                        self.settings_state.is_testing_ollama = false;
                        match res {
                            Ok(models) => {
                                self.settings_state.ollama_test_status = Some(format!(
                                    "✓ Bağlantı Başarılı ({} model yüklü)",
                                    models.len()
                                ));
                            }
                            Err(err) => {
                                self.settings_state.ollama_test_status = Some(format!("✗ Hata: {}", err));
                            }
                        }
                    }
                    SettingsMessage::FontSizeChanged(size) => {
                        self.config.font_size = size;
                        self.save_config();
                        let msg = HudMessage::UpdateStyle {
                            font_size: size,
                            opacity: (self.config.hud_opacity as f32) / 100.0,
                            duration_ms: self.config.hud_duration_ms,
                        };
                        return Task::perform(
                            async move {
                                let client = HudIpcClient::default_client();
                                let _ = client.send(&msg).await;
                            },
                            |_| cosmic::Action::None,
                        );
                    }
                    SettingsMessage::OpacityChanged(opacity) => {
                        self.config.hud_opacity = opacity;
                        self.save_config();
                        let msg = HudMessage::UpdateStyle {
                            font_size: self.config.font_size,
                            opacity: (opacity as f32) / 100.0,
                            duration_ms: self.config.hud_duration_ms,
                        };
                        return Task::perform(
                            async move {
                                let client = HudIpcClient::default_client();
                                let _ = client.send(&msg).await;
                            },
                            |_| cosmic::Action::None,
                        );
                    }
                    SettingsMessage::DurationChanged(dur) => {
                        self.config.hud_duration_ms = dur;
                        self.save_config();
                        let msg = HudMessage::UpdateStyle {
                            font_size: self.config.font_size,
                            opacity: (self.config.hud_opacity as f32) / 100.0,
                            duration_ms: dur,
                        };
                        return Task::perform(
                            async move {
                                let client = HudIpcClient::default_client();
                                let _ = client.send(&msg).await;
                            },
                            |_| cosmic::Action::None,
                        );
                    }
                    SettingsMessage::TestHudRequested => {
                        let bin = overlay_binary_path();
                        let _ = std::process::Command::new(bin).arg("--hud").spawn();
                        let dur = self.config.hud_duration_ms;
                        return Task::perform(
                            async move {
                                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                let client = HudIpcClient::default_client();
                                let _ = client
                                    .send(&HudMessage::ShowSubtitle {
                                        text: "🎮 Test Altyazısı: cosmic-ext-lingua aktif ve hazır!".to_string(),
                                        original: Some("Test Subtitle".to_string()),
                                        duration_ms: dur,
                                    })
                                    .await;
                            },
                            |_| cosmic::Action::None,
                        );
                    }
                    SettingsMessage::GlossarySourceChanged(s) => {
                        self.settings_state.new_glossary_source = s;
                    }
                    SettingsMessage::GlossaryTargetChanged(t) => {
                        self.settings_state.new_glossary_target = t;
                    }
                    SettingsMessage::AddGlossaryTerm => {
                        let src = self.settings_state.new_glossary_source.trim().to_string();
                        let trg = self.settings_state.new_glossary_target.trim().to_string();
                        if !src.is_empty() && !trg.is_empty() {
                            self.config.glossary.insert(src, trg);
                            self.settings_state.new_glossary_source.clear();
                            self.settings_state.new_glossary_target.clear();
                            self.save_config();
                        }
                    }
                    SettingsMessage::RemoveGlossaryTerm(term) => {
                        self.config.glossary.remove(&term);
                        self.save_config();
                    }
                    SettingsMessage::Close => {
                        if let Some(id) = self.settings_window.take() {
                            return cosmic::iced::window::close(id);
                        }
                    }
                }
            }

            Message::PresetSelected(preset) => {
                self.config.preset = preset.as_str().to_string();
                self.save_config();
                if let Some(ref session) = self.capture_session {
                    let region = CaptureRegion::new(preset, self.config.custom_roi);
                    session.update_region(region);
                }
            }

            Message::SelectRoiRequested => {
                let bin = overlay_binary_path();
                return Task::perform(
                    async move {
                        let output = tokio::process::Command::new(bin)
                            .arg("--selector")
                            .output()
                            .await;
                        if let Ok(out) = output {
                            let stdout_str = String::from_utf8_lossy(&out.stdout);
                            for line in stdout_str.lines() {
                                if let Some(json_str) = line.strip_prefix("ROI_SELECTED:") {
                                    if let Ok(roi) = serde_json::from_str::<Roi>(json_str) {
                                        return Message::RoiSelected(roi);
                                    }
                                }
                            }
                        }
                        Message::RoiSelectionCancelled
                    },
                    cosmic::Action::from,
                );
            }

            Message::RoiSelected(roi) => {
                info!("Selected capture ROI: {:?}", roi);
                self.config.custom_roi = Some(roi);
                self.config.preset = RoiPreset::Custom.as_str().to_string();
                self.save_config();
                if let Some(ref session) = self.capture_session {
                    let region = CaptureRegion::new(RoiPreset::Custom, Some(roi));
                    session.update_region(region);
                }
            }

            Message::RoiSelectionCancelled => {
                info!("ROI selection was cancelled");
            }

            Message::ToggleCapture(true) => {
                self.capture_active = true;

                // Ensure the HUD overlay is running
                let bin = overlay_binary_path();
                let _ = std::process::Command::new(bin)
                    .arg("--hud")
                    .spawn();

                let preset = RoiPreset::from_str_name(&self.config.preset);
                let region = CaptureRegion::new(preset, self.config.custom_roi);
                return Task::perform(
                    async move {
                        match CaptureSession::start(None, false, region).await {
                            Ok(session) => Message::CaptureStarted(session),
                            Err(err) => {
                                error!("Failed to start capture session: {err}");
                                Message::CaptureFailed(err.to_string())
                            }
                        }
                    },
                    cosmic::Action::from,
                );
            }

            Message::CaptureStarted(session) => {
                info!("Capture session active, starting translation orchestrator");
                let frame_rx = session.frame_receiver.clone();
                self.capture_session = Some(session);

                let ocr_backend = Arc::new(MockOcrBackend::new("Game Subtitle"));
                let translation_backend = Arc::new(OllamaTranslationBackend::new(
                    &self.config.ollama_endpoint,
                    &self.config.ollama_model,
                ));
                let orchestrator_config = OrchestratorConfig {
                    src_lang: self.config.source_lang.clone(),
                    target_lang: self.config.target_lang.clone(),
                    ..Default::default()
                };

                let handle = spawn_orchestrator(
                    frame_rx,
                    FrameDebouncer::new(
                        2,
                        std::time::Duration::from_millis(self.config.settle_duration_ms),
                        std::time::Duration::from_millis(2000),
                    ),
                    ocr_backend,
                    translation_backend,
                    orchestrator_config,
                );
                self.orchestrator = Some(handle);
            }

            Message::CaptureFailed(err) => {
                error!("Capture start failed: {err}");
                self.capture_active = false;
            }

            Message::ToggleCapture(false) => {
                self.capture_active = false;
                if let Some(orch) = self.orchestrator.take() {
                    orch.stop();
                }
                if let Some(mut session) = self.capture_session.take() {
                    let _ = session.stop();
                }
                return Task::perform(
                    async move {
                        let client = HudIpcClient::default_client();
                        let _ = client.send(&HudMessage::ClearSubtitle).await;
                        Message::HudCleared
                    },
                    cosmic::Action::from,
                );
            }

            Message::HudCleared => {}
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        self.core
            .applet
            .icon_button("accessories-dictionary-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if Some(id) == self.settings_window {
            return self.settings_state.view(&self.config).map(Message::Settings);
        }

        let status_text = if self.capture_active {
            fl!("active")
        } else {
            fl!("paused")
        };

        let current_preset = RoiPreset::from_str_name(&self.config.preset);
        let preset_label = current_preset.label();

        let roi_desc = match current_preset {
            RoiPreset::Custom => {
                if let Some(r) = self.config.custom_roi {
                    format!("{}x{} @ ({}, {})", r.width, r.height, r.x, r.y)
                } else {
                    "Özel Alan Seçilmedi".to_string()
                }
            }
            _ => preset_label.to_string(),
        };

        let mut preset_buttons = row![].spacing(4);
        for p in [RoiPreset::Bottom25, RoiPreset::Bottom33, RoiPreset::FullScreen] {
            let label = match p {
                RoiPreset::Bottom25 => "Alt %25",
                RoiPreset::Bottom33 => "Alt %33",
                RoiPreset::FullScreen => "Tam Ekran",
                _ => "",
            };
            let btn = if p == current_preset {
                button::suggested(label)
            } else {
                button::standard(label)
            };
            preset_buttons = preset_buttons.push(btn.on_press(Message::PresetSelected(p)));
        }

        let content_list = widget::list_column()
            .add(widget::settings::item(
                fl!("status"),
                widget::text(status_text),
            ))
            .add(widget::settings::item(
                fl!("example-row"),
                widget::toggler(self.capture_active).on_toggle(Message::ToggleCapture),
            ))
            .add(widget::settings::item(
                "Şablon",
                preset_buttons,
            ))
            .add(widget::settings::item(
                "Aktif Bölge",
                widget::text(roi_desc),
            ))
            .add(
                widget::button::standard(fl!("select-roi"))
                    .on_press(Message::SelectRoiRequested),
            )
            .add(
                widget::button::suggested(fl!("settings"))
                    .on_press(Message::OpenSettings),
            );

        self.core.applet.popup_container(content_list).into()
    }
}

fn overlay_binary_path() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("cosmic-translator-overlay");
            if candidate.exists() {
                return candidate;
            }
        }
    }
    let target_debug = std::path::PathBuf::from("target/debug/cosmic-translator-overlay");
    if target_debug.exists() {
        return target_debug;
    }
    std::path::PathBuf::from("cosmic-translator-overlay")
}
