//! Application model shared by the panel applet (`--applet`) and the standalone
//! settings window. `update` routes each message to a handler in a submodule.

mod capture;
mod changes;
mod dashboard;
mod ocr;
mod popup;
mod reload;
mod shortcuts;
mod translation;

use std::sync::Arc;

use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::{window::Id, Task};
use cosmic::prelude::*;
use cosmic::widget;
use tracing::warn;
use translator_core::ipc::{HudMessage, SelectorOutput};
use translator_core::ocr::OcrBackend;
use translator_core::orchestrator::{OrchestratorEvent, OrchestratorHandle};
use translator_core::region::RoiPreset;
use translator_core::shortcuts::ShortcutAction;
use translator_core::CaptureSession;

pub use changes::ConfigDelta;
pub use dashboard::PreviewMode;
pub use ocr::OcrStatus;
pub use reload::RELOAD_EXIT_CODE;

use crate::config::Config;
use crate::fl;
use crate::history::DialogueBacklog;
use crate::preview::PreviewImages;
use crate::services::{self, HudService, RegionKind};
use crate::settings::{self, Request, SettingsMessage, SettingsState};

#[derive(Default)]
pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    config: Config,
    config_handler: Option<cosmic_config::Config>,
    settings_state: SettingsState,
    capture_active: bool,
    /// Portal and PipeWire setup is in progress (the portal dialog may be open).
    capture_starting: bool,
    /// Why the last capture start failed; shown until the next attempt.
    capture_error: Option<String>,
    hud_visible: bool,
    ocr_backend: Option<Arc<dyn OcrBackend>>,
    ocr_status: OcrStatus,
    /// Incremented for every backend (re)build so results of superseded builds are dropped.
    ocr_generation: u64,
    /// Incremented for every capture session; identifies the preview subscription.
    capture_generation: u64,
    orchestrator: Option<OrchestratorHandle>,
    capture_session: Option<CaptureSession>,
    preview_mode: PreviewMode,
    last_raw_preview: Option<cosmic::iced::core::image::Handle>,
    last_proc_preview: Option<cosmic::iced::core::image::Handle>,
    last_ocr_text: String,
    last_translated_text: String,
    pub backlog: DialogueBacklog,
    hud: HudService,
}

#[derive(Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    OpenSettings,
    ConfigChanged(Box<Config>),
    Settings(SettingsMessage),
    ToggleCapture(bool),
    ChangeCaptureTarget,
    SelectRoiRequested,
    /// Selector process exited; `None` when cancelled or failed.
    RegionSelectorFinished(RegionKind, Option<SelectorOutput>),
    PresetSelected(RoiPreset),
    CaptureStarted(CaptureSession),
    OcrBackendLoaded {
        generation: u64,
        result: Result<Arc<dyn OcrBackend>, String>,
    },
    CaptureFailed {
        error: String,
        /// The user closed the portal dialog; not shown as an error.
        cancelled: bool,
    },
    ShortcutActivated(ShortcutAction),
    OneShotTranslate,
    OneShotCompleted(Option<(String, String)>),
    TogglePreviewMode(PreviewMode),
    CloseApp,
    OrchestratorEventReceived(OrchestratorEvent),
    PreviewUpdated(PreviewImages),
    /// SIGHUP in the panel applet; see `reload`.
    ReloadRequested,
}

// `CaptureSession` owns the PipeWire thread and cannot be cloned. iced only clones
// messages produced by widgets, which never carry a session.
impl Clone for Message {
    fn clone(&self) -> Self {
        match self {
            Self::TogglePopup => Self::TogglePopup,
            Self::PopupClosed(id) => Self::PopupClosed(*id),
            Self::OpenSettings => Self::OpenSettings,
            Self::ConfigChanged(c) => Self::ConfigChanged(c.clone()),
            Self::Settings(m) => Self::Settings(m.clone()),
            Self::ToggleCapture(v) => Self::ToggleCapture(*v),
            Self::ChangeCaptureTarget => Self::ChangeCaptureTarget,
            Self::SelectRoiRequested => Self::SelectRoiRequested,
            Self::RegionSelectorFinished(kind, output) => {
                Self::RegionSelectorFinished(*kind, *output)
            }
            Self::PresetSelected(p) => Self::PresetSelected(*p),
            Self::CaptureStarted(_) => Self::CaptureFailed {
                error: "Cannot clone active session".into(),
                cancelled: false,
            },
            Self::OcrBackendLoaded { generation, result } => Self::OcrBackendLoaded {
                generation: *generation,
                result: result.clone(),
            },
            Self::CaptureFailed { error, cancelled } => Self::CaptureFailed {
                error: error.clone(),
                cancelled: *cancelled,
            },
            Self::ShortcutActivated(a) => Self::ShortcutActivated(a.clone()),
            Self::OneShotTranslate => Self::OneShotTranslate,
            Self::OneShotCompleted(s) => Self::OneShotCompleted(s.clone()),
            Self::TogglePreviewMode(m) => Self::TogglePreviewMode(*m),
            Self::CloseApp => Self::CloseApp,
            Self::OrchestratorEventReceived(e) => Self::OrchestratorEventReceived(e.clone()),
            Self::PreviewUpdated(p) => Self::PreviewUpdated(p.clone()),
            Self::ReloadRequested => Self::ReloadRequested,
        }
    }
}

impl AppModel {
    fn save_config(&self) {
        if let Some(handler) = &self.config_handler {
            if let Err(err) = self.config.write_entry(handler) {
                warn!("Failed to save config: {err}");
            }
        }
    }

    pub fn is_standalone(&self) -> bool {
        !std::env::args().any(|arg| arg == "--applet")
    }

    fn capture_status_text(&self) -> String {
        if self.capture_starting {
            fl!("starting")
        } else if self.capture_active {
            fl!("active")
        } else {
            fl!("paused")
        }
    }

    /// Active preset, or the custom region's geometry.
    fn region_description(&self) -> String {
        match RoiPreset::from_str_name(&self.config.preset) {
            RoiPreset::Custom => self
                .config
                .custom_roi
                .map_or_else(|| fl!("custom-roi-unset"), settings::capture::format_roi),
            preset => settings::capture::preset_label(preset),
        }
    }

    /// Failure reason of the last capture start, with a pointer to the log files.
    fn capture_error_view(&self) -> Option<Element<'_, Message>> {
        let error = self.capture_error.as_deref()?;
        let mut column = widget::column![
            widget::text(fl!("capture-failed")).size(14),
            widget::text(error)
                .size(12)
                .wrapping(cosmic::iced::widget::text::Wrapping::Word),
        ]
        .spacing(2);
        if let Some(dir) = translator_core::logging::log_dir() {
            let dir = dir.display().to_string();
            column = column.push(widget::text(fl!("see-logs", path = dir)).size(12));
        }
        Some(column.into())
    }

    fn settings_message(&mut self, message: SettingsMessage) -> Task<cosmic::Action<Message>> {
        let old = self.config.clone();
        let update = self.settings_state.update(
            message,
            settings::Context {
                config: &mut self.config,
                backlog: &mut self.backlog,
                hud: &self.hud,
            },
        );
        let page_task = update
            .task
            .map(|msg| cosmic::Action::from(Message::Settings(msg)));

        let config_task = if self.config == old {
            Task::none()
        } else {
            self.save_config();
            self.apply_config_change(&old)
        };

        let request_task = match update.request {
            Some(Request::SelectRegion(kind)) => self.run_region_selector(kind),
            Some(Request::Close) if self.is_standalone() => self.close_app(),
            Some(Request::Close) | None => Task::none(),
        };

        Task::batch([page_task, config_task, request_task])
    }

    fn open_settings_window(&mut self) -> Task<cosmic::Action<Message>> {
        if self.is_standalone() {
            return Task::none();
        }
        let spawn = Task::future(async { services::process::spawn_settings_window() }).discard();
        Task::batch([self.close_popup(), spawn])
    }

    fn close_app(&mut self) -> Task<cosmic::Action<Message>> {
        self.stop_capture_session();
        let close = match self.core.main_window_id() {
            Some(id) => cosmic::iced::window::close(id),
            None => Task::none(),
        };
        Task::batch([self.hud.send_task(HudMessage::Quit), close])
    }
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
                Err(err) => {
                    warn!("Config unavailable, using defaults: {err}");
                    (None, Config::default())
                }
            };

        // `AppModel` implements `Drop`, so struct update syntax cannot be used.
        let mut app = AppModel::default();
        app.core = core;
        app.config = config;
        app.config_handler = config_handler;
        app.hud_visible = true;

        (app, Task::none())
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Self::Message> {
        // Only the standalone dashboard shows previews; the panel applet skips the work.
        let preview = match &self.capture_session {
            Some(session) if self.is_standalone() => crate::preview::subscription(
                crate::preview::PreviewSource {
                    session_id: self.capture_generation,
                    binarize: self.config.binarize,
                    blur_subtraction: self.config.blur_subtraction,
                    frames: session.frame_receiver.clone(),
                },
                Message::PreviewUpdated,
            ),
            _ => cosmic::iced::Subscription::none(),
        };
        // The standalone window is not managed by the panel; SIGHUP keeps its default.
        let reload = if self.is_standalone() {
            cosmic::iced::Subscription::none()
        } else {
            reload::subscription()
        };
        cosmic::iced::Subscription::batch([
            preview,
            reload,
            shortcuts::global_shortcuts_subscription(),
            translation::orchestrator_events_subscription(),
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
            Message::TogglePopup => self.toggle_popup(),
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
                Task::none()
            }
            Message::OpenSettings => self.open_settings_window(),
            Message::ConfigChanged(new_config) => {
                let old = std::mem::replace(&mut self.config, *new_config);
                self.apply_config_change(&old)
            }
            Message::Settings(message) => self.settings_message(message),
            Message::CloseApp => self.close_app(),
            Message::ShortcutActivated(action) => self.shortcut_activated(action),
            Message::PresetSelected(preset) => {
                self.edit_config(|config| config.preset = preset.as_str().to_string())
            }
            Message::ChangeCaptureTarget => self.change_capture_target(),
            Message::SelectRoiRequested => self.run_region_selector(RegionKind::Capture),
            Message::RegionSelectorFinished(kind, output) => self.region_selected(kind, output),
            Message::PreviewUpdated(images) => {
                self.set_previews(images);
                Task::none()
            }
            Message::OrchestratorEventReceived(event) => {
                self.orchestrator_event(event);
                Task::none()
            }
            Message::OneShotTranslate => self.one_shot_translate(),
            Message::OneShotCompleted(result) => {
                if let Some((text, translated)) = result {
                    self.last_ocr_text = text;
                    self.last_translated_text = translated;
                }
                Task::none()
            }
            Message::TogglePreviewMode(mode) => {
                self.preview_mode = mode;
                Task::none()
            }
            Message::ToggleCapture(true) => self.start_capture(),
            Message::ToggleCapture(false) => self.stop_capture(),
            Message::CaptureStarted(session) => self.capture_started(session),
            Message::CaptureFailed { error, cancelled } => {
                self.capture_failed(error, cancelled);
                Task::none()
            }
            Message::OcrBackendLoaded { generation, result } => {
                self.ocr_backend_loaded(generation, result)
            }
            Message::ReloadRequested => self.reload(),
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        if self.is_standalone() {
            return self.view_dashboard();
        }
        self.core
            .applet
            .icon_button("accessories-dictionary-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if Some(id) == self.popup {
            return self.view_popup();
        }
        widget::text("").into()
    }
}

impl Drop for AppModel {
    fn drop(&mut self) {
        if let Some(mut session) = self.capture_session.take() {
            let _ = session.stop();
        }
        self.hud.quit_detached();
    }
}
