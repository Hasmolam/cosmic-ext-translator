use std::sync::Arc;
use crate::config::Config;
use crate::fl;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{window::Id, Limits, Task};
use cosmic::prelude::*;
use cosmic::widget;
use tracing::{error, info};
use translator_core::ipc::{HudIpcClient, HudMessage};
use translator_core::ocr::MockOcrBackend;
use translator_core::orchestrator::{
    spawn_orchestrator, OrchestratorConfig, OrchestratorHandle,
};
use translator_core::pipeline::debouncer::FrameDebouncer;
use translator_core::translation::MockTranslationBackend;
use translator_core::{CaptureSession, Roi};

#[derive(Default)]
pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    #[allow(dead_code)]
    config: Config,
    capture_active: bool,
    current_roi: Option<Roi>,
    orchestrator: Option<OrchestratorHandle>,
    capture_session: Option<CaptureSession>,
}

#[derive(Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    ToggleCapture(bool),
    SelectRoiRequested,
    RoiSelected(Roi),
    RoiSelectionCancelled,
    CaptureStarted(CaptureSession),
    CaptureFailed(String),
    HudCleared,
}

impl Clone for Message {
    fn clone(&self) -> Self {
        match self {
            Self::TogglePopup => Self::TogglePopup,
            Self::PopupClosed(id) => Self::PopupClosed(*id),
            Self::ToggleCapture(v) => Self::ToggleCapture(*v),
            Self::SelectRoiRequested => Self::SelectRoiRequested,
            Self::RoiSelected(r) => Self::RoiSelected(*r),
            Self::RoiSelectionCancelled => Self::RoiSelectionCancelled,
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
        let app = AppModel {
            core,
            config: cosmic_config::Config::new(Self::APP_ID, Config::VERSION)
                .map(|context| match Config::get_entry(&context) {
                    Ok(config) => config,
                    Err((_errors, config)) => config,
                })
                .unwrap_or_default(),
            capture_active: false,
            ..Default::default()
        };

        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
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
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(1080.0);
                    get_popup(popup_settings)
                };
            }

            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
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
                self.current_roi = Some(roi);
                if let Some(ref session) = self.capture_session {
                    session.update_roi(Some(roi));
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

                let roi = self.current_roi;
                return Task::perform(
                    async move {
                        match CaptureSession::start(None, false, roi).await {
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
                let translation_backend = Arc::new(MockTranslationBackend::new("TR"));
                let config = OrchestratorConfig::default();

                let handle = spawn_orchestrator(
                    frame_rx,
                    FrameDebouncer::default(),
                    ocr_backend,
                    translation_backend,
                    config,
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

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let status_text = if self.capture_active {
            fl!("active")
        } else {
            fl!("paused")
        };

        let roi_desc = if let Some(r) = self.current_roi {
            format!("{}x{} @ ({}, {})", r.width, r.height, r.x, r.y)
        } else {
            "Full Screen".to_string()
        };

        let content_list = widget::list_column()
            .add(widget::settings::item(
                fl!("status"),
                widget::text(status_text),
            ))
            .add(widget::settings::item(
                "Region",
                widget::text(roi_desc),
            ))
            .add(widget::settings::item(
                fl!("example-row"),
                widget::toggler(self.capture_active).on_toggle(Message::ToggleCapture),
            ))
            .add(
                widget::button::standard(fl!("select-roi"))
                    .on_press(Message::SelectRoiRequested),
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
