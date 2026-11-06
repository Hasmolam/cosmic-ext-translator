use crate::config::Config;
use crate::fl;
use crate::history::DialogueBacklog;
use crate::settings_dialog::{SettingsMessage, SettingsState};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{window::Id, Length, Limits, Task};
use cosmic::prelude::*;
use cosmic::widget::{self, button, row};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use translator_core::capture::pipewire_thread::CapturedFrame;
use translator_core::ipc::{HudIpcClient, HudMessage, HudPosition, HudStatus};
use translator_core::ocr::{create_ocr_backend, OcrBackend, OcrEngineChoice, OrientationMode};
use translator_core::orchestrator::{
    spawn_orchestrator, OrchestratorConfig, OrchestratorEvent, OrchestratorHandle,
};
use translator_core::pipeline::debouncer::FrameDebouncer;
use translator_core::pipeline::preprocess::PreprocessConfig;
use translator_core::region::{CaptureRegion, RoiPreset};
use translator_core::shortcuts::{GlobalShortcutsManager, ShortcutAction};
use translator_core::translation::{
    DeepLTranslationBackend, GoogleTranslateBackend, OllamaTranslationBackend,
    OpenAiTranslationBackend, TranslationBackend,
};
use translator_core::{normalize_source_code, CaptureSession, Roi};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewMode {
    #[default]
    RawCropped,
    Preprocessed,
}

/// Availability of the OCR backend for the current capture session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum OcrStatus {
    /// No capture session; no backend needed.
    #[default]
    Idle,
    Loading,
    Ready,
    /// Backend construction failed; carries a user-facing reason.
    Unavailable(String),
}

#[derive(Default)]
pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    config: Config,
    config_handler: Option<cosmic_config::Config>,
    settings_state: SettingsState,
    capture_active: bool,
    hud_visible: bool,
    ocr_backend: Option<Arc<dyn OcrBackend>>,
    ocr_status: OcrStatus,
    /// Incremented for every backend (re)build so results of superseded builds are dropped.
    ocr_generation: u64,
    orchestrator: Option<OrchestratorHandle>,
    capture_session: Option<CaptureSession>,
    preview_mode: PreviewMode,
    last_raw_preview: Option<cosmic::iced::core::image::Handle>,
    last_proc_preview: Option<cosmic::iced::core::image::Handle>,
    last_ocr_text: String,
    last_translated_text: String,
    pub backlog: DialogueBacklog,
}

impl AppModel {
    fn save_config(&self) {
        if let Some(ref handler) = self.config_handler {
            let _ = self.config.write_entry(handler);
        }
    }

    /// Builds a new OCR backend for the configured source language.
    ///
    /// The running orchestrator is stopped first so frames are not read with a backend
    /// for the wrong script while the new one loads.
    fn rebuild_ocr_backend(&mut self) -> Task<cosmic::Action<Message>> {
        if let Some(old) = self.orchestrator.take() {
            old.stop();
        }
        self.ocr_backend = None;
        self.ocr_status = OcrStatus::Loading;
        self.ocr_generation = self.ocr_generation.wrapping_add(1);
        let generation = self.ocr_generation;
        let src_lang = self.config.source_lang.clone();
        let orientation = OrientationMode::from_code(&self.config.ocr_orientation);
        let engine = OcrEngineChoice::from_code(&self.config.ocr_engine);

        Task::perform(
            async move {
                let result = create_ocr_backend(&src_lang, orientation, engine)
                    .await
                    .map_err(|err| err.to_string());
                Message::OcrBackendLoaded { generation, result }
            },
            cosmic::Action::from,
        )
    }

    /// Invalidates any in-flight backend build and drops the current backend.
    fn reset_ocr_backend(&mut self) {
        self.ocr_generation = self.ocr_generation.wrapping_add(1);
        self.ocr_backend = None;
        self.ocr_status = OcrStatus::Idle;
    }

    /// True if switching from `old` to the current config requires a new OCR backend.
    fn ocr_settings_changed(&self, old: &Config) -> bool {
        ocr_settings_differ(old, &self.config)
    }

    fn restart_orchestrator_if_active(&mut self) {
        if !self.capture_active {
            return;
        }
        let (Some(ref session), Some(ref ocr_backend)) = (&self.capture_session, &self.ocr_backend)
        else {
            return;
        };

        if let Some(old) = self.orchestrator.take() {
            old.stop();
        }

        let frame_rx = session.frame_receiver.clone();
        let translation_backend: Arc<dyn TranslationBackend> = match self.config.engine.as_str() {
            "ollama" => Arc::new(OllamaTranslationBackend::new(
                &self.config.ollama_endpoint,
                &self.config.ollama_model,
            )),
            "deepl" => Arc::new(DeepLTranslationBackend::new(&self.config.deepl_api_key)),
            "openai" => Arc::new(OpenAiTranslationBackend::new(
                &self.config.openai_endpoint,
                &self.config.openai_api_key,
                &self.config.openai_model,
            )),
            _ => Arc::new(GoogleTranslateBackend::new()),
        };

        let orchestrator_config = OrchestratorConfig {
            src_lang: normalize_source_code(&self.config.source_lang),
            target_lang: self.config.target_lang.clone(),
            glossary: self.config.glossary.clone(),
            hud_duration_ms: self.config.hud_duration_ms,
            preprocess: PreprocessConfig {
                binarize: self.config.binarize,
                blur_subtraction: self.config.blur_subtraction,
                ..Default::default()
            },
            ..Default::default()
        };

        let handle = spawn_orchestrator(
            frame_rx,
            FrameDebouncer::new(
                2,
                std::time::Duration::from_millis(self.config.settle_duration_ms),
                std::time::Duration::from_millis(2000),
            ),
            Arc::clone(ocr_backend),
            translation_backend,
            orchestrator_config,
        );
        let mut drain_handle = handle.clone();
        let bus = event_bus().clone();
        tokio::spawn(async move {
            while let Some(event) = drain_handle.next_event().await {
                let _ = bus.send(event);
            }
        });
        self.orchestrator = Some(handle);
    }

    pub fn is_standalone(&self) -> bool {
        !std::env::args().any(|arg| arg == "--applet")
    }

    fn update_previews(&mut self, frame: &CapturedFrame) {
        let w = frame.image.width();
        let h = frame.image.height();
        if w == 0 || h == 0 {
            return;
        }

        let raw_bytes = frame.image.clone().into_raw();
        self.last_raw_preview = Some(cosmic::iced::core::image::Handle::from_rgba(
            w, h, raw_bytes,
        ));

        let preprocess_cfg = PreprocessConfig {
            binarize: self.config.binarize,
            blur_subtraction: self.config.blur_subtraction,
            ..Default::default()
        };
        let gray = translator_core::pipeline::preprocess::preprocess_for_ocr(
            &frame.image,
            &preprocess_cfg,
        );
        let mut gray_rgba = image::RgbaImage::new(gray.width(), gray.height());
        for (x, y, p) in gray.enumerate_pixels() {
            gray_rgba.put_pixel(x, y, image::Rgba([p[0], p[0], p[0], 255]));
        }
        self.last_proc_preview = Some(cosmic::iced::core::image::Handle::from_rgba(
            gray.width(),
            gray.height(),
            gray_rgba.into_raw(),
        ));
    }

    fn view_standalone_dashboard(&self) -> Element<'_, Message> {
        let status_text = if self.capture_active {
            fl!("active")
        } else {
            fl!("paused")
        };

        let current_preset = RoiPreset::from_str_name(&self.config.preset);
        let preset_label = match current_preset {
            RoiPreset::Bottom25 => fl!("preset-bottom25-full"),
            RoiPreset::Bottom33 => fl!("preset-bottom33-full"),
            RoiPreset::FullScreen => fl!("preset-fullscreen-full"),
            RoiPreset::Custom => fl!("preset-custom-full"),
        };

        let roi_desc = match current_preset {
            RoiPreset::Custom => {
                if let Some(r) = self.config.custom_roi {
                    format!("{}x{} @ ({}, {})", r.width, r.height, r.x, r.y)
                } else {
                    fl!("custom-roi-unset")
                }
            }
            _ => preset_label.clone(),
        };

        let header = row![
            widget::text("cosmic-ext-translator")
                .size(20)
                .wrapping(cosmic::iced::widget::text::Wrapping::Word),
            cosmic::iced::widget::space::horizontal(),
            widget::text(format!("{}: {status_text}", fl!("status"))).size(14),
            widget::toggler(self.capture_active).on_toggle(Message::ToggleCapture),
        ]
        .spacing(12)
        .align_y(cosmic::iced::Alignment::Center);

        let quick_actions = row![
            button::suggested(fl!("select-target")).on_press(Message::ChangeCaptureTarget),
            button::standard(fl!("select-roi")).on_press(Message::SelectRoiRequested),
            button::standard(fl!("translate-now")).on_press(Message::OneShotTranslate),
            widget::text(format!("{}: {roi_desc}", fl!("region"))).size(13),
        ]
        .spacing(10)
        .align_y(cosmic::iced::Alignment::Center);

        let preview_handle = match self.preview_mode {
            PreviewMode::RawCropped => self.last_raw_preview.as_ref(),
            PreviewMode::Preprocessed => self.last_proc_preview.as_ref(),
        };

        let preview_widget: Element<'_, Message> = if let Some(handle) = preview_handle {
            widget::container(
                widget::image(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fixed(110.0)),
            )
            .width(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        } else {
            widget::container(widget::text(fl!("prompt-select-target")).size(13))
                .padding(14)
                .center_x(Length::Fill)
                .into()
        };

        let preview_selector = row![
            widget::text(fl!("live-inspection-title")).size(13),
            cosmic::iced::widget::space::horizontal(),
            if self.preview_mode == PreviewMode::RawCropped {
                button::suggested(fl!("raw-crop"))
            } else {
                button::standard(fl!("raw-crop"))
            }
            .on_press(Message::TogglePreviewMode(PreviewMode::RawCropped)),
            if self.preview_mode == PreviewMode::Preprocessed {
                button::suggested(fl!("ocr-binarize"))
            } else {
                button::standard(fl!("ocr-binarize"))
            }
            .on_press(Message::TogglePreviewMode(PreviewMode::Preprocessed)),
        ]
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center);

        let ocr_feedback = widget::column![
            row![
                widget::text(format!("{}: ", fl!("last-detected-ocr"))).size(13),
                widget::text(if self.last_ocr_text.is_empty() {
                    "-"
                } else {
                    &self.last_ocr_text
                })
                .size(13)
                .wrapping(cosmic::iced::widget::text::Wrapping::Word),
            ],
            row![
                widget::text(format!("{}: ", fl!("translation-output"))).size(13),
                widget::text(if self.last_translated_text.is_empty() {
                    "-"
                } else {
                    &self.last_translated_text
                })
                .size(13)
                .wrapping(cosmic::iced::widget::text::Wrapping::Word),
            ]
        ]
        .spacing(4);

        let inspection_card = widget::container(
            widget::column![
                preview_selector,
                widget::divider::horizontal::default(),
                preview_widget,
                widget::divider::horizontal::default(),
                ocr_feedback,
            ]
            .spacing(8)
            .padding(10),
        )
        .class(cosmic::theme::Container::Card);

        let settings_content = self
            .settings_state
            .view(&self.config, &self.backlog)
            .map(Message::Settings);

        let top_section = widget::column![
            header,
            widget::divider::horizontal::default(),
            quick_actions,
            widget::divider::horizontal::default(),
            inspection_card,
            widget::divider::horizontal::default(),
        ]
        .spacing(10);

        let layout = widget::column![top_section, settings_content,]
            .spacing(10)
            .padding(16);

        widget::container(layout)
            .width(cosmic::iced::Length::Fill)
            .height(cosmic::iced::Length::Fill)
            .class(cosmic::theme::Container::WindowBackground)
            .into()
    }
}

#[derive(Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    OpenSettings,
    CloseSettingsWindow(Id),
    ConfigChanged(Box<Config>),
    Settings(SettingsMessage),
    ToggleCapture(bool),
    ChangeCaptureTarget,
    SelectRoiRequested,
    RoiSelected(Roi),
    RoiSelectedWithSurface {
        roi: Roi,
        surface_width: Option<u32>,
        surface_height: Option<u32>,
    },
    RoiSelectionCancelled,
    SelectIgnoreRoiRequested,
    IgnoreRoiSelectedWithSurface {
        roi: Roi,
        surface_width: Option<u32>,
        surface_height: Option<u32>,
    },
    IgnoreRoiSelected(Roi),
    IgnoreRoiSelectionCancelled,
    ClearIgnoreRoi,
    PresetSelected(RoiPreset),
    CaptureStarted(CaptureSession),
    OcrBackendLoaded {
        generation: u64,
        result: Result<Arc<dyn OcrBackend>, String>,
    },
    CaptureFailed(String),
    ShortcutActivated(ShortcutAction),
    OneShotTranslate,
    OneShotCompleted(Option<(String, String)>),
    TogglePreviewMode(PreviewMode),
    CloseApp,
    HudCleared,
    OrchestratorEventReceived(OrchestratorEvent),
    ClearHistory,
    CopyHistoryEntry(String),
}

impl Clone for Message {
    fn clone(&self) -> Self {
        match self {
            Self::TogglePopup => Self::TogglePopup,
            Self::PopupClosed(id) => Self::PopupClosed(*id),
            Self::OpenSettings => Self::OpenSettings,
            Self::CloseSettingsWindow(id) => Self::CloseSettingsWindow(*id),
            Self::ConfigChanged(c) => Self::ConfigChanged(c.clone()),
            Self::Settings(m) => Self::Settings(m.clone()),
            Self::ToggleCapture(v) => Self::ToggleCapture(*v),
            Self::ChangeCaptureTarget => Self::ChangeCaptureTarget,
            Self::SelectRoiRequested => Self::SelectRoiRequested,
            Self::RoiSelected(r) => Self::RoiSelected(*r),
            Self::RoiSelectedWithSurface {
                roi,
                surface_width,
                surface_height,
            } => Self::RoiSelectedWithSurface {
                roi: *roi,
                surface_width: *surface_width,
                surface_height: *surface_height,
            },
            Self::RoiSelectionCancelled => Self::RoiSelectionCancelled,
            Self::SelectIgnoreRoiRequested => Self::SelectIgnoreRoiRequested,
            Self::IgnoreRoiSelectedWithSurface {
                roi,
                surface_width,
                surface_height,
            } => Self::IgnoreRoiSelectedWithSurface {
                roi: *roi,
                surface_width: *surface_width,
                surface_height: *surface_height,
            },
            Self::IgnoreRoiSelected(r) => Self::IgnoreRoiSelected(*r),
            Self::IgnoreRoiSelectionCancelled => Self::IgnoreRoiSelectionCancelled,
            Self::ClearIgnoreRoi => Self::ClearIgnoreRoi,
            Self::PresetSelected(p) => Self::PresetSelected(*p),
            Self::CaptureStarted(_) => Self::CaptureFailed("Cannot clone active session".into()),
            Self::OcrBackendLoaded { generation, result } => Self::OcrBackendLoaded {
                generation: *generation,
                result: result.clone(),
            },
            Self::CaptureFailed(s) => Self::CaptureFailed(s.clone()),
            Self::ShortcutActivated(a) => Self::ShortcutActivated(a.clone()),
            Self::OneShotTranslate => Self::OneShotTranslate,
            Self::OneShotCompleted(s) => Self::OneShotCompleted(s.clone()),
            Self::TogglePreviewMode(m) => Self::TogglePreviewMode(*m),
            Self::CloseApp => Self::CloseApp,
            Self::HudCleared => Self::HudCleared,
            Self::OrchestratorEventReceived(e) => Self::OrchestratorEventReceived(e.clone()),
            Self::ClearHistory => Self::ClearHistory,
            Self::CopyHistoryEntry(s) => Self::CopyHistoryEntry(s.clone()),
        }
    }
}

static EVENT_BUS: std::sync::OnceLock<tokio::sync::broadcast::Sender<OrchestratorEvent>> =
    std::sync::OnceLock::new();

fn event_bus() -> &'static tokio::sync::broadcast::Sender<OrchestratorEvent> {
    EVENT_BUS.get_or_init(|| {
        let (tx, _rx) = tokio::sync::broadcast::channel(256);
        tx
    })
}

fn orchestrator_events_subscription() -> cosmic::iced::Subscription<Message> {
    struct OrchestratorSub;

    cosmic::iced::Subscription::run_with(std::any::TypeId::of::<OrchestratorSub>(), |_| {
        cosmic::iced::stream::channel(
            64,
            |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                use cosmic::iced::futures::SinkExt;
                let mut rx = event_bus().subscribe();
                while let Ok(event) = rx.recv().await {
                    if output
                        .send(Message::OrchestratorEventReceived(event))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            },
        )
    })
}

fn global_shortcuts_subscription() -> cosmic::iced::Subscription<Message> {
    struct GlobalShortcutsSub;

    cosmic::iced::Subscription::run_with(std::any::TypeId::of::<GlobalShortcutsSub>(), |_| {
        cosmic::iced::stream::channel(
            16,
            |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                use cosmic::iced::futures::SinkExt;
                match GlobalShortcutsManager::register(None).await {
                    Ok((_manager, mut shortcut_rx)) => {
                        while let Some(action) = shortcut_rx.recv().await {
                            if output
                                .send(Message::ShortcutActivated(action))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                    Err(err) => {
                        debug!("Global shortcuts portal unavailable or declined: {err}");
                        cosmic::iced::futures::future::pending::<()>().await;
                    }
                }
            },
        )
    })
}

fn hud_position_from_str(pos: &str) -> HudPosition {
    match pos {
        "top" => HudPosition::Top,
        "center" => HudPosition::Center,
        _ => HudPosition::Bottom,
    }
}

async fn ensure_hud_running_and_styled(
    font_size: u16,
    opacity: u8,
    duration_ms: u64,
    position: &str,
    offset_y: u32,
) {
    let client = HudIpcClient::default_client();
    if !client.is_running().await {
        let bin = overlay_binary_path();
        let _ = std::process::Command::new(bin).arg("--hud").spawn();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let hud_pos = hud_position_from_str(position);
    let _ = client
        .send(&HudMessage::UpdateStyle {
            font_size,
            opacity: (opacity as f32) / 100.0,
            duration_ms,
            position: hud_pos,
            offset_y,
        })
        .await;
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = crate::config::APP_ID;

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        mut core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let is_standalone = !std::env::args().any(|arg| arg == "--applet");
        if is_standalone {
            core.set_header_title("cosmic-ext-translator".to_string());
        }
        let (config_handler, config) =
            match cosmic_config::Config::new(Self::APP_ID, Config::VERSION) {
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
            config,
            config_handler,
            settings_state: SettingsState::new(),
            capture_active: false,
            hud_visible: true,
            ocr_backend: None,
            ocr_status: OcrStatus::Idle,
            ocr_generation: 0,
            orchestrator: None,
            capture_session: None,
            preview_mode: PreviewMode::default(),
            last_raw_preview: None,
            last_proc_preview: None,
            last_ocr_text: String::new(),
            last_translated_text: String::new(),
            backlog: DialogueBacklog::default(),
        };

        (app, Task::none())
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Self::Message> {
        cosmic::iced::Subscription::batch([
            global_shortcuts_subscription(),
            orchestrator_events_subscription(),
            self.core.watch_config::<Config>(Self::APP_ID).map(|u| {
                if !u.errors.is_empty() {
                    warn!("Errors updating config: {:?}", u.errors);
                }
                Message::ConfigChanged(Box::new(u.config))
            }),
        ])
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        if Some(id) == self.popup {
            Some(Message::PopupClosed(id))
        } else if self.is_standalone() && self.core.main_window_id() == Some(id) {
            Some(Message::CloseApp)
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
                    let Some(main_id) = self.core.main_window_id() else {
                        return Task::none();
                    };
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self
                        .core
                        .applet
                        .get_popup_settings(main_id, new_id, None, None, None);
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
                if self.is_standalone() {
                    return Task::none();
                }
                let close_popup_task = if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    Task::none()
                };

                let spawn_task = Task::perform(
                    async move {
                        let bin = applet_binary_path();
                        let mut cmd = std::process::Command::new(bin);
                        if let Err(e) = cmd.spawn() {
                            error!("Failed to launch settings application: {e}");
                        }
                    },
                    |_| cosmic::Action::None,
                );

                return Task::batch([close_popup_task, spawn_task]);
            }

            Message::ConfigChanged(new_config) => {
                let old_config = std::mem::replace(&mut self.config, *new_config);
                let ocr_changed = self.ocr_settings_changed(&old_config);
                if let Some(ref session) = self.capture_session {
                    let preset = RoiPreset::from_str_name(&self.config.preset);
                    let region =
                        CaptureRegion::new(preset, self.config.custom_roi, self.config.ignore_roi);
                    session.update_region(region);
                }
                let ocr_task = if ocr_changed && self.capture_session.is_some() {
                    info!(
                        "OCR settings changed (source '{}', orientation '{}', engine '{}'), rebuilding OCR backend",
                        self.config.source_lang, self.config.ocr_orientation, self.config.ocr_engine
                    );
                    self.rebuild_ocr_backend()
                } else {
                    self.restart_orchestrator_if_active();
                    Task::none()
                };

                let font_size = self.config.font_size;
                let opacity = self.config.hud_opacity;
                let duration_ms = self.config.hud_duration_ms;
                let position = self.config.hud_position.clone();
                let offset_y = self.config.hud_offset_y;
                let hud_task = Task::perform(
                    async move {
                        ensure_hud_running_and_styled(
                            font_size,
                            opacity,
                            duration_ms,
                            &position,
                            offset_y,
                        )
                        .await;
                    },
                    |_| cosmic::Action::None,
                );
                return Task::batch([ocr_task, hud_task]);
            }

            Message::CloseApp => {
                if let Some(orch) = self.orchestrator.take() {
                    orch.stop();
                }
                if let Some(mut session) = self.capture_session.take() {
                    let _ = session.stop();
                }
                let close_task = if let Some(id) = self.core.main_window_id() {
                    cosmic::iced::window::close(id)
                } else {
                    Task::none()
                };
                let quit_task = Task::perform(
                    async move {
                        let client = HudIpcClient::default_client();
                        let _ = client.send(&HudMessage::Quit).await;
                    },
                    |_| cosmic::Action::None,
                );
                return Task::batch([quit_task, close_task]);
            }

            Message::CloseSettingsWindow(_id) => {
                return Task::none();
            }

            Message::ShortcutActivated(action) => match action {
                ShortcutAction::SelectRoi => {
                    return self.update(Message::SelectRoiRequested);
                }
                ShortcutAction::ToggleTranslation => {
                    return self.update(Message::ToggleCapture(!self.capture_active));
                }
                ShortcutAction::ToggleHud => {
                    self.hud_visible = !self.hud_visible;
                    let visible = self.hud_visible;
                    return Task::perform(
                        async move {
                            let client = HudIpcClient::default_client();
                            let _ = client.send(&HudMessage::SetVisible { visible }).await;
                        },
                        |_| cosmic::Action::None,
                    );
                }
                ShortcutAction::TranslateCurrentScreen => {
                    return self.update(Message::OneShotTranslate);
                }
                ShortcutAction::Custom(id) => {
                    info!("Unhandled custom global shortcut: {id}");
                }
            },

            Message::Settings(settings_msg) => match settings_msg {
                SettingsMessage::TabSelected(tab) => {
                    self.settings_state.active_tab = tab;
                }
                SettingsMessage::PresetSelected(preset) => {
                    self.config.preset = preset.as_str().to_string();
                    self.save_config();
                    if let Some(ref session) = self.capture_session {
                        let region = CaptureRegion::new(
                            preset,
                            self.config.custom_roi,
                            self.config.ignore_roi,
                        );
                        session.update_region(region);
                    }
                }
                SettingsMessage::EngineSelected(engine) => {
                    self.config.engine = engine;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::SelectCustomRoi => {
                    return self.update(Message::SelectRoiRequested);
                }
                SettingsMessage::BinarizeToggled(bin) => {
                    self.config.binarize = bin;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::BlurSubtractionToggled(val) => {
                    self.config.blur_subtraction = val;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::HudPositionChanged(pos) => {
                    self.config.hud_position = pos.clone();
                    self.save_config();
                    let hud_pos = hud_position_from_str(&pos);
                    let offset_y = self.config.hud_offset_y;
                    return Task::perform(
                        async move {
                            let client = HudIpcClient::default_client();
                            let _ = client
                                .send(&HudMessage::SetPosition {
                                    position: hud_pos,
                                    offset_y,
                                })
                                .await;
                        },
                        |_| cosmic::Action::None,
                    );
                }
                SettingsMessage::HudPersistentToggled(persistent) => {
                    self.config.hud_persistent = persistent;
                    if persistent {
                        self.config.hud_duration_ms = 0;
                    } else if self.config.hud_duration_ms == 0 {
                        self.config.hud_duration_ms = 6000;
                    }
                    self.save_config();
                    self.restart_orchestrator_if_active();
                    let msg = HudMessage::UpdateStyle {
                        font_size: self.config.font_size,
                        opacity: (self.config.hud_opacity as f32) / 100.0,
                        duration_ms: self.config.hud_duration_ms,
                        position: hud_position_from_str(&self.config.hud_position),
                        offset_y: self.config.hud_offset_y,
                    };
                    return Task::perform(
                        async move {
                            let client = HudIpcClient::default_client();
                            let _ = client.send(&msg).await;
                        },
                        |_| cosmic::Action::None,
                    );
                }
                SettingsMessage::HudOffsetYChanged(offset) => {
                    self.config.hud_offset_y = offset;
                    self.save_config();
                    let msg = HudMessage::UpdateStyle {
                        font_size: self.config.font_size,
                        opacity: (self.config.hud_opacity as f32) / 100.0,
                        duration_ms: self.config.hud_duration_ms,
                        position: hud_position_from_str(&self.config.hud_position),
                        offset_y: offset,
                    };
                    return Task::perform(
                        async move {
                            let client = HudIpcClient::default_client();
                            let _ = client.send(&msg).await;
                        },
                        |_| cosmic::Action::None,
                    );
                }
                SettingsMessage::DeepLApiKeyChanged(key) => {
                    self.config.deepl_api_key = key;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::OpenAiEndpointChanged(ep) => {
                    self.config.openai_endpoint = ep;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::OpenAiApiKeyChanged(key) => {
                    self.config.openai_api_key = key;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::OpenAiModelChanged(m) => {
                    self.config.openai_model = m;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::SettleDurationChanged(ms) => {
                    self.config.settle_duration_ms = ms;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::OllamaEndpointChanged(ep) => {
                    self.config.ollama_endpoint = ep;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::OllamaModelChanged(model) => {
                    self.config.ollama_model = model;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::TargetLangChanged(lang) => {
                    self.config.target_lang = lang;
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::SourceLangChanged(lang) => {
                    let old = self.config.clone();
                    self.config.source_lang = lang;
                    self.save_config();
                    if self.ocr_settings_changed(&old) && self.capture_session.is_some() {
                        return self.rebuild_ocr_backend();
                    }
                }
                SettingsMessage::OcrOrientationChanged(mode) => {
                    let old = self.config.clone();
                    self.config.ocr_orientation = mode;
                    self.save_config();
                    if self.ocr_settings_changed(&old) && self.capture_session.is_some() {
                        return self.rebuild_ocr_backend();
                    }
                }
                SettingsMessage::OcrEngineChanged(engine) => {
                    let old = self.config.clone();
                    self.config.ocr_engine = engine;
                    self.save_config();
                    if self.ocr_settings_changed(&old) && self.capture_session.is_some() {
                        return self.rebuild_ocr_backend();
                    }
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
                        |res| {
                            cosmic::Action::from(Message::Settings(
                                SettingsMessage::OllamaTestResult(res),
                            ))
                        },
                    );
                }
                SettingsMessage::OllamaTestResult(res) => {
                    self.settings_state.is_testing_ollama = false;
                    match res {
                        Ok(models) => {
                            self.settings_state.ollama_test_status = Some(format!(
                                "✓ {}",
                                fl!("connection-success", count = models.len())
                            ));
                        }
                        Err(err) => {
                            self.settings_state.ollama_test_status = Some(format!(
                                "✗ {}",
                                fl!("connection-error", error = err.as_str())
                            ));
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
                        position: hud_position_from_str(&self.config.hud_position),
                        offset_y: self.config.hud_offset_y,
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
                        position: hud_position_from_str(&self.config.hud_position),
                        offset_y: self.config.hud_offset_y,
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
                    self.restart_orchestrator_if_active();
                    let msg = HudMessage::UpdateStyle {
                        font_size: self.config.font_size,
                        opacity: (self.config.hud_opacity as f32) / 100.0,
                        duration_ms: dur,
                        position: hud_position_from_str(&self.config.hud_position),
                        offset_y: self.config.hud_offset_y,
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
                    let font_size = self.config.font_size;
                    let opacity = self.config.hud_opacity;
                    let dur = self.config.hud_duration_ms;
                    let pos = self.config.hud_position.clone();
                    let offset_y = self.config.hud_offset_y;
                    return Task::perform(
                        async move {
                            ensure_hud_running_and_styled(font_size, opacity, dur, &pos, offset_y)
                                .await;
                            let client = HudIpcClient::default_client();
                            let test_text = fl!("test-subtitle-content");
                            let _ = client
                                .send(&HudMessage::ShowSubtitle {
                                    text: test_text,
                                    original: Some("Test Subtitle".to_string()),
                                    duration_ms: dur,
                                })
                                .await;
                        },
                        |_| cosmic::Action::None,
                    );
                }
                SettingsMessage::StartInteractivePositioning => {
                    let font_size = self.config.font_size;
                    let opacity = self.config.hud_opacity;
                    let dur = self.config.hud_duration_ms;
                    let pos = self.config.hud_position.clone();
                    let offset_y = self.config.hud_offset_y;
                    return Task::perform(
                        async move {
                            ensure_hud_running_and_styled(font_size, opacity, dur, &pos, offset_y)
                                .await;
                            let client = HudIpcClient::default_client();
                            let _ = client
                                .send(&HudMessage::SetEditMode { edit_mode: true })
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
                        self.restart_orchestrator_if_active();
                    }
                }
                SettingsMessage::RemoveGlossaryTerm(term) => {
                    self.config.glossary.remove(&term);
                    self.save_config();
                    self.restart_orchestrator_if_active();
                }
                SettingsMessage::SelectIgnoreRoi => {
                    return self.update(Message::SelectIgnoreRoiRequested);
                }
                SettingsMessage::ClearIgnoreRoi => {
                    return self.update(Message::ClearIgnoreRoi);
                }
                SettingsMessage::HistorySearchChanged(q) => {
                    self.settings_state.history_search_query = q;
                }
                SettingsMessage::ClearHistory => {
                    return self.update(Message::ClearHistory);
                }
                SettingsMessage::CopyHistoryEntry(text) => {
                    return self.update(Message::CopyHistoryEntry(text));
                }
                SettingsMessage::Close => {
                    if self.is_standalone() {
                        return self.update(Message::CloseApp);
                    }
                }
            },

            Message::PresetSelected(preset) => {
                self.config.preset = preset.as_str().to_string();
                self.save_config();
                if let Some(ref session) = self.capture_session {
                    let region =
                        CaptureRegion::new(preset, self.config.custom_roi, self.config.ignore_roi);
                    session.update_region(region);
                }
            }

            Message::ChangeCaptureTarget => {
                info!("User requested selecting a new target window or screen");
                self.config.restore_token = None;
                self.save_config();

                if let Some(orch) = self.orchestrator.take() {
                    orch.stop();
                }
                if let Some(mut session) = self.capture_session.take() {
                    let _ = session.stop();
                }

                return self.update(Message::ToggleCapture(true));
            }

            Message::SelectRoiRequested => {
                let bin = overlay_binary_path();
                let requester = self
                    .capture_session
                    .as_ref()
                    .map(|s| s.snapshot_requester());

                return Task::perform(
                    async move {
                        let full_snapshot = if let Some(req) = requester {
                            req.get_full_snapshot().await
                        } else {
                            None
                        };

                        let preview_path = if let Some(img) = full_snapshot {
                            let path = std::env::temp_dir().join(format!(
                                "cosmic-ext-translator-full-{}.bmp",
                                std::process::id()
                            ));
                            if img.save_with_format(&path, image::ImageFormat::Bmp).is_ok() {
                                Some(path)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let mut cmd = tokio::process::Command::new(bin);
                        cmd.arg("--selector");
                        if let Some(ref p) = preview_path {
                            cmd.arg(p);
                        }
                        let output = cmd.output().await;
                        if let Some(ref p) = preview_path {
                            let _ = std::fs::remove_file(p);
                        }
                        if let Ok(out) = output {
                            let stdout_str = String::from_utf8_lossy(&out.stdout);
                            for line in stdout_str.lines() {
                                if let Some(json_str) = line.strip_prefix("ROI_SELECTED:") {
                                    #[derive(serde::Deserialize)]
                                    struct SelectedPayload {
                                        x: u32,
                                        y: u32,
                                        width: u32,
                                        height: u32,
                                        surface_width: Option<u32>,
                                        surface_height: Option<u32>,
                                    }
                                    if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str)
                                    {
                                        let roi = Roi::new(p.x, p.y, p.width, p.height);
                                        return Message::RoiSelectedWithSurface {
                                            roi,
                                            surface_width: p.surface_width,
                                            surface_height: p.surface_height,
                                        };
                                    }
                                }
                            }
                        }
                        Message::RoiSelectionCancelled
                    },
                    cosmic::Action::from,
                );
            }

            Message::RoiSelectedWithSurface {
                roi,
                surface_width,
                surface_height,
            } => {
                let effective_roi = if let (Some(sw), Some(sh), Some(ref session)) =
                    (surface_width, surface_height, &self.capture_session)
                {
                    if let Some(frame) = session.frame_receiver.borrow().as_ref() {
                        let scaled =
                            roi.scale_to_frame(sw, sh, frame.original_width, frame.original_height);
                        info!(
                            "Scaled ROI from surface ({sw}x{sh}) to frame ({}x{}): {:?}",
                            frame.original_width, frame.original_height, scaled
                        );
                        scaled
                    } else {
                        roi
                    }
                } else {
                    roi
                };
                return self.update(Message::RoiSelected(effective_roi));
            }

            Message::RoiSelected(roi) => {
                info!("Selected capture ROI: {:?}", roi);
                self.config.custom_roi = Some(roi);
                self.config.preset = RoiPreset::Custom.as_str().to_string();
                self.save_config();
                if let Some(ref session) = self.capture_session {
                    let region =
                        CaptureRegion::new(RoiPreset::Custom, Some(roi), self.config.ignore_roi);
                    session.update_region(region);
                }
            }

            Message::RoiSelectionCancelled => {
                info!("ROI selection was cancelled");
            }

            Message::SelectIgnoreRoiRequested => {
                let bin = overlay_binary_path();
                let requester = self
                    .capture_session
                    .as_ref()
                    .map(|s| s.snapshot_requester());

                return Task::perform(
                    async move {
                        let full_snapshot = if let Some(req) = requester {
                            req.get_full_snapshot().await
                        } else {
                            None
                        };

                        let preview_path = if let Some(img) = full_snapshot {
                            let path = std::env::temp_dir().join(format!(
                                "cosmic-ext-translator-ignore-{}.bmp",
                                std::process::id()
                            ));
                            if img.save_with_format(&path, image::ImageFormat::Bmp).is_ok() {
                                Some(path)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let mut cmd = tokio::process::Command::new(bin);
                        cmd.arg("--selector");
                        if let Some(ref p) = preview_path {
                            cmd.arg(p);
                        }
                        let output = cmd.output().await;
                        if let Some(ref p) = preview_path {
                            let _ = std::fs::remove_file(p);
                        }
                        if let Ok(out) = output {
                            let stdout_str = String::from_utf8_lossy(&out.stdout);
                            for line in stdout_str.lines() {
                                if let Some(json_str) = line.strip_prefix("ROI_SELECTED:") {
                                    #[derive(serde::Deserialize)]
                                    struct SelectedPayload {
                                        x: u32,
                                        y: u32,
                                        width: u32,
                                        height: u32,
                                        surface_width: Option<u32>,
                                        surface_height: Option<u32>,
                                    }
                                    if let Ok(p) = serde_json::from_str::<SelectedPayload>(json_str)
                                    {
                                        let roi = Roi::new(p.x, p.y, p.width, p.height);
                                        return Message::IgnoreRoiSelectedWithSurface {
                                            roi,
                                            surface_width: p.surface_width,
                                            surface_height: p.surface_height,
                                        };
                                    }
                                }
                            }
                        }
                        Message::IgnoreRoiSelectionCancelled
                    },
                    cosmic::Action::from,
                );
            }

            Message::IgnoreRoiSelectedWithSurface {
                roi,
                surface_width,
                surface_height,
            } => {
                let effective_roi = if let (Some(sw), Some(sh), Some(ref session)) =
                    (surface_width, surface_height, &self.capture_session)
                {
                    if let Some(frame) = session.frame_receiver.borrow().as_ref() {
                        let scaled =
                            roi.scale_to_frame(sw, sh, frame.original_width, frame.original_height);
                        info!(
                            "Scaled Ignore ROI from surface ({sw}x{sh}) to frame ({}x{}): {:?}",
                            frame.original_width, frame.original_height, scaled
                        );
                        scaled
                    } else {
                        roi
                    }
                } else {
                    roi
                };
                return self.update(Message::IgnoreRoiSelected(effective_roi));
            }

            Message::IgnoreRoiSelected(roi) => {
                info!("Selected ignore ROI: {:?}", roi);
                self.config.ignore_roi = Some(roi);
                self.save_config();
                if let Some(ref session) = self.capture_session {
                    let preset = RoiPreset::from_str_name(&self.config.preset);
                    let region = CaptureRegion::new(preset, self.config.custom_roi, Some(roi));
                    session.update_region(region);
                }
                self.restart_orchestrator_if_active();
            }

            Message::IgnoreRoiSelectionCancelled => {
                info!("Ignore ROI selection was cancelled");
            }

            Message::ClearIgnoreRoi => {
                info!("Cleared ignore ROI");
                self.config.ignore_roi = None;
                self.save_config();
                if let Some(ref session) = self.capture_session {
                    let preset = RoiPreset::from_str_name(&self.config.preset);
                    let region = CaptureRegion::new(preset, self.config.custom_roi, None);
                    session.update_region(region);
                }
                self.restart_orchestrator_if_active();
            }

            Message::ClearHistory => {
                self.backlog.clear();
            }

            Message::CopyHistoryEntry(text) => {
                return cosmic::iced::clipboard::write(text);
            }

            Message::OrchestratorEventReceived(event) => match event {
                OrchestratorEvent::TextDetected { raw } => {
                    self.last_ocr_text = raw;
                }
                OrchestratorEvent::TranslationReady {
                    original,
                    translated,
                    cached: _,
                } => {
                    let parsed = translator_core::translation::parse_speaker_tags(
                        &original,
                        &self.config.glossary,
                    );
                    let speaker = parsed.resolved_speaker.or(parsed.raw_speaker);
                    let source_text = if !parsed.dialogue.is_empty() {
                        parsed.dialogue
                    } else {
                        original.clone()
                    };
                    self.last_ocr_text = original;
                    self.last_translated_text = translated.clone();
                    self.backlog.push(speaker, source_text, translated);
                }
                _ => {}
            },

            Message::OneShotTranslate => {
                let Some(ref session) = self.capture_session else {
                    info!(
                        "One-shot translation requested but capture is inactive, starting capture"
                    );
                    return self.update(Message::ToggleCapture(true));
                };

                let frame_opt = session.frame_receiver.borrow().clone();
                let Some(frame) = frame_opt else {
                    info!("No frame available yet for one-shot translate");
                    return Task::none();
                };

                self.update_previews(&frame);

                let ocr_opt = self.ocr_backend.clone();
                let Some(ocr) = ocr_opt else {
                    return Task::none();
                };

                let engine = self.config.engine.clone();
                let src_lang = normalize_source_code(&self.config.source_lang);
                let target_lang = self.config.target_lang.clone();
                let binarize = self.config.binarize;
                let blur_subtraction = self.config.blur_subtraction;
                let glossary = self.config.glossary.clone();
                let hud_duration = self.config.hud_duration_ms;
                let ollama_ep = self.config.ollama_endpoint.clone();
                let ollama_model = self.config.ollama_model.clone();
                let deepl_key = self.config.deepl_api_key.clone();
                let openai_ep = self.config.openai_endpoint.clone();
                let openai_key = self.config.openai_api_key.clone();
                let openai_model = self.config.openai_model.clone();

                return Task::perform(
                    async move {
                        let preprocess_cfg = PreprocessConfig {
                            binarize,
                            blur_subtraction,
                            ..Default::default()
                        };
                        let gray = translator_core::pipeline::preprocess::preprocess_for_ocr(
                            &frame.image,
                            &preprocess_cfg,
                        );
                        let dynamic = image::DynamicImage::ImageLuma8(gray);

                        let text = match ocr.recognize(&dynamic).await {
                            Ok(t) => t.trim().to_string(),
                            Err(e) => {
                                warn!("One-shot OCR failed: {e}");
                                return Message::OneShotCompleted(None);
                            }
                        };

                        if text.is_empty() {
                            return Message::OneShotCompleted(None);
                        }

                        let backend: Arc<dyn TranslationBackend> = match engine.as_str() {
                            "ollama" => {
                                Arc::new(OllamaTranslationBackend::new(ollama_ep, ollama_model))
                            }
                            "deepl" => Arc::new(DeepLTranslationBackend::new(deepl_key)),
                            "openai" => Arc::new(OpenAiTranslationBackend::new(
                                openai_ep,
                                openai_key,
                                openai_model,
                            )),
                            _ => Arc::new(GoogleTranslateBackend::new()),
                        };

                        let mut ctx = translator_core::translation::TranslationContext::default();
                        for (k, v) in glossary {
                            ctx.set_glossary_term(k, v);
                        }

                        match backend
                            .translate(&text, &src_lang, &target_lang, &ctx)
                            .await
                        {
                            Ok(translated) => {
                                let client = HudIpcClient::default_client();
                                let _ = client
                                    .send(&HudMessage::ShowSubtitle {
                                        text: translated.clone(),
                                        original: Some(text.clone()),
                                        duration_ms: hud_duration,
                                    })
                                    .await;
                                Message::OneShotCompleted(Some((text, translated)))
                            }
                            Err(err) => {
                                warn!("One-shot translation failed: {err}");
                                Message::OneShotCompleted(None)
                            }
                        }
                    },
                    cosmic::Action::from,
                );
            }

            Message::OneShotCompleted(opt) => {
                if let Some((raw, tr)) = opt {
                    self.last_ocr_text = raw;
                    self.last_translated_text = tr;
                }
            }

            Message::TogglePreviewMode(mode) => {
                self.preview_mode = mode;
            }

            Message::ToggleCapture(true) => {
                self.capture_active = true;

                let font_size = self.config.font_size;
                let opacity = self.config.hud_opacity;
                let duration_ms = self.config.hud_duration_ms;
                let position = self.config.hud_position.clone();
                let offset_y = self.config.hud_offset_y;
                let restore_token = self.config.restore_token.clone();
                let preset = RoiPreset::from_str_name(&self.config.preset);
                let region =
                    CaptureRegion::new(preset, self.config.custom_roi, self.config.ignore_roi);

                return Task::perform(
                    async move {
                        ensure_hud_running_and_styled(
                            font_size,
                            opacity,
                            duration_ms,
                            &position,
                            offset_y,
                        )
                        .await;
                        let session =
                            match CaptureSession::start(restore_token.as_deref(), true, region)
                                .await
                            {
                                Ok(s) => s,
                                Err(err) => {
                                    error!("Failed to start capture session: {err}");
                                    return Message::CaptureFailed(err.to_string());
                                }
                            };
                        Message::CaptureStarted(session)
                    },
                    cosmic::Action::from,
                );
            }

            Message::CaptureStarted(session) => {
                info!("Capture session active, loading OCR backend");
                if session.stream_info.restore_token != self.config.restore_token {
                    self.config.restore_token = session.stream_info.restore_token.clone();
                    self.save_config();
                }

                if let Some(frame) = session.frame_receiver.borrow().as_ref() {
                    self.update_previews(frame);
                }

                self.capture_session = Some(session);
                return self.rebuild_ocr_backend();
            }

            Message::OcrBackendLoaded { generation, result } => {
                if generation != self.ocr_generation {
                    debug!("Discarding superseded OCR backend build {generation}");
                    return Task::none();
                }
                let status = match result {
                    Ok(backend) => {
                        info!("OCR backend ready, starting translation orchestrator");
                        self.ocr_backend = Some(backend);
                        self.ocr_status = OcrStatus::Ready;
                        self.restart_orchestrator_if_active();
                        None
                    }
                    Err(reason) => {
                        error!("OCR backend unavailable: {reason}");
                        self.ocr_backend = None;
                        self.ocr_status = OcrStatus::Unavailable(reason.clone());
                        Some(HudStatus::OcrUnavailable { reason })
                    }
                };
                return Task::perform(
                    async move {
                        let client = HudIpcClient::default_client();
                        if let Err(err) = client.send(&HudMessage::SetHudStatus { status }).await {
                            debug!("HUD status send failed: {err}");
                        }
                    },
                    |_| cosmic::Action::None,
                );
            }

            Message::CaptureFailed(err) => {
                error!("Capture start failed: {err}");
                self.capture_active = false;
                if self.config.restore_token.is_some() {
                    warn!(
                        "Resetting invalid restore token after capture failure to prevent lockouts"
                    );
                    self.config.restore_token = None;
                    self.save_config();
                }
            }

            Message::ToggleCapture(false) => {
                self.capture_active = false;
                if let Some(orch) = self.orchestrator.take() {
                    orch.stop();
                }
                if let Some(mut session) = self.capture_session.take() {
                    let _ = session.stop();
                }
                self.reset_ocr_backend();
                return Task::perform(
                    async move {
                        let client = HudIpcClient::default_client();
                        let _ = client.send(&HudMessage::ClearSubtitle).await;
                        let _ = client
                            .send(&HudMessage::SetHudStatus { status: None })
                            .await;
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
        if self.is_standalone() {
            return self.view_standalone_dashboard();
        }

        self.core
            .applet
            .icon_button("accessories-dictionary-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if Some(id) == self.popup {
            let status_text = if self.capture_active {
                fl!("active")
            } else {
                fl!("paused")
            };

            let current_preset = RoiPreset::from_str_name(&self.config.preset);
            let preset_label = match current_preset {
                RoiPreset::Bottom25 => fl!("preset-bottom25-full"),
                RoiPreset::Bottom33 => fl!("preset-bottom33-full"),
                RoiPreset::FullScreen => fl!("preset-fullscreen-full"),
                RoiPreset::Custom => fl!("preset-custom-full"),
            };

            let roi_desc = match current_preset {
                RoiPreset::Custom => {
                    if let Some(r) = self.config.custom_roi {
                        format!("{}x{} @ ({}, {})", r.width, r.height, r.x, r.y)
                    } else {
                        fl!("custom-roi-unset")
                    }
                }
                _ => preset_label.clone(),
            };

            let btn_b25 = if current_preset == RoiPreset::Bottom25 {
                button::suggested(fl!("preset-bottom25-short"))
            } else {
                button::standard(fl!("preset-bottom25-short"))
            }
            .width(cosmic::iced::Length::Fill)
            .on_press(Message::PresetSelected(RoiPreset::Bottom25));

            let btn_b33 = if current_preset == RoiPreset::Bottom33 {
                button::suggested(fl!("preset-bottom33-short"))
            } else {
                button::standard(fl!("preset-bottom33-short"))
            }
            .width(cosmic::iced::Length::Fill)
            .on_press(Message::PresetSelected(RoiPreset::Bottom33));

            let btn_full = if current_preset == RoiPreset::FullScreen {
                button::suggested(fl!("preset-fullscreen-short"))
            } else {
                button::standard(fl!("preset-fullscreen-short"))
            }
            .width(cosmic::iced::Length::Fill)
            .on_press(Message::PresetSelected(RoiPreset::FullScreen));

            let preset_controls = widget::column![
                widget::text(fl!("preset")).size(14),
                row![btn_b25, btn_b33]
                    .spacing(6)
                    .width(cosmic::iced::Length::Fill),
                row![btn_full].width(cosmic::iced::Length::Fill),
            ]
            .spacing(6);

            let content_list = widget::list_column()
                .add(widget::settings::item(
                    fl!("status"),
                    widget::text(status_text),
                ))
                .add(widget::settings::item(
                    fl!("enable-translation"),
                    widget::toggler(self.capture_active).on_toggle(Message::ToggleCapture),
                ));

            let content_list = match &self.ocr_status {
                OcrStatus::Loading => content_list.add(widget::settings::item(
                    fl!("ocr-status"),
                    widget::text(fl!("ocr-loading")),
                )),
                OcrStatus::Unavailable(reason) => {
                    let detail = widget::column![
                        widget::text(fl!("ocr-unavailable")).size(14),
                        widget::text(reason.as_str()).size(12),
                    ]
                    .spacing(2);
                    content_list.add(widget::settings::item_row(vec![detail.into()]))
                }
                OcrStatus::Idle | OcrStatus::Ready => content_list,
            };

            let content_list = content_list
                .add(preset_controls)
                .add(widget::settings::item(
                    fl!("active-region"),
                    widget::text(roi_desc),
                ))
                .add(
                    widget::button::suggested(fl!("select-target"))
                        .on_press(Message::ChangeCaptureTarget),
                )
                .add(
                    widget::button::standard(fl!("select-roi"))
                        .on_press(Message::SelectRoiRequested),
                )
                .add(
                    widget::button::standard(fl!("translate-now"))
                        .on_press(Message::OneShotTranslate),
                )
                .add(widget::button::suggested(fl!("settings")).on_press(Message::OpenSettings));

            return self
                .core
                .applet
                .popup_container(widget::container(content_list).width(380))
                .into();
        }

        widget::text("").into()
    }
}

impl Drop for AppModel {
    fn drop(&mut self) {
        if let Some(mut session) = self.capture_session.take() {
            let _ = session.stop();
        }
        let client = HudIpcClient::default_client();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            if let Ok(runtime) = rt {
                runtime.block_on(async {
                    let _ = client.send(&HudMessage::Quit).await;
                });
            }
        });
    }
}

/// Compares the config fields that determine which OCR backend is built.
fn ocr_settings_differ(a: &Config, b: &Config) -> bool {
    normalize_source_code(&a.source_lang) != normalize_source_code(&b.source_lang)
        || OrientationMode::from_code(&a.ocr_orientation)
            != OrientationMode::from_code(&b.ocr_orientation)
        || OcrEngineChoice::from_code(&a.ocr_engine) != OcrEngineChoice::from_code(&b.ocr_engine)
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

fn applet_binary_path() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if exe.is_file() {
            return exe;
        }
    }
    std::path::PathBuf::from("cosmic-applet-translator")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_rebuild_triggers() {
        let base = Config::default();
        let same_lang_alias = Config {
            source_lang: "AUTO".to_string(),
            ..Config::default()
        };
        assert!(!ocr_settings_differ(&base, &same_lang_alias));

        let japanese = Config {
            source_lang: "ja".to_string(),
            ..Config::default()
        };
        assert!(ocr_settings_differ(&base, &japanese));

        let japanese_alias = Config {
            source_lang: "jp".to_string(),
            ..Config::default()
        };
        assert!(!ocr_settings_differ(&japanese, &japanese_alias));

        let vertical = Config {
            ocr_orientation: "vertical".to_string(),
            ..japanese.clone()
        };
        assert!(ocr_settings_differ(&japanese, &vertical));

        let tesseract = Config {
            ocr_engine: "tesseract".to_string(),
            ..japanese.clone()
        };
        assert!(ocr_settings_differ(&japanese, &tesseract));
        let auto_alias = Config {
            ocr_engine: String::new(),
            ..japanese.clone()
        };
        assert!(!ocr_settings_differ(&japanese, &auto_alias));

        let unrelated = Config {
            font_size: 40,
            ..japanese.clone()
        };
        assert!(!ocr_settings_differ(&japanese, &unrelated));
    }
}
