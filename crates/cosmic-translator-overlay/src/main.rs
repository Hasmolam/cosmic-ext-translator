use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, get_layer_surface, set_anchor, set_input_zone,
    set_keyboard_interactivity, set_margin, set_size, Anchor, KeyboardInteractivity, Layer,
};
use cosmic::iced::window::Id;
use cosmic::iced::{Length, Task};
use cosmic::widget;
use cosmic::Element;
use cosmic_translator_overlay::hud::{HudUiMessage, SubtitleHudState};
use cosmic_translator_overlay::selector::{SelectorMessage, SelectorProgram};
use std::env;
use std::io::Write;
use std::time::Duration;
use translator_protocol::{
    default_socket_path, selector, HudIpcServer, HudMessage, HudPosition, SelectorOutput,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppMode {
    Hud,
    Selector {
        preview_path: Option<std::path::PathBuf>,
    },
}

pub struct OverlayApp {
    core: cosmic::Core,
    mode: AppMode,
    hud: SubtitleHudState,
    selector_program: SelectorProgram,
    selector_id: Id,
    hud_id: Id,
    subtitle_seq: u64,
}

#[derive(Debug, Clone)]
pub enum Message {
    Selector(SelectorMessage),
    Hud(HudMessage),
    HudUi(HudUiMessage),
    ClearSubtitle(u64),
    Close,
}

fn hud_ipc_subscription() -> cosmic::iced::Subscription<Message> {
    struct HudIpcSub;
    cosmic::iced::Subscription::run_with(std::any::TypeId::of::<HudIpcSub>(), |_| {
        cosmic::iced::stream::channel(
            16,
            |mut output: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                use cosmic::iced::futures::SinkExt;
                let server = HudIpcServer::default_server();
                if let Ok(mut server_rx) = server.start().await {
                    while let Some(msg) = server_rx.recv().await {
                        if output.send(Message::Hud(msg)).await.is_err() {
                            break;
                        }
                    }
                }
            },
        )
    })
}

impl OverlayApp {
    /// Applies the current HUD layer settings to the existing HUD surface.
    ///
    /// `get_layer_surface` with the id of an existing surface is ignored by the platform
    /// layer ("already exists"), so position, size and input changes have to be sent as
    /// individual layer-shell requests.
    fn reconfigure_hud(&self) -> Task<cosmic::Action<Message>> {
        let settings = self.hud.layer_settings();
        let id = self.hud_id;
        let (width, height) = settings.size.unwrap_or((None, None));
        let margin = settings.margin;
        Task::batch([
            set_anchor(id, settings.anchor),
            set_margin(id, margin.top, margin.right, margin.bottom, margin.left),
            set_size(id, width, height),
            set_keyboard_interactivity(id, settings.keyboard_interactivity),
            set_input_zone(id, settings.input_zone),
        ])
    }
}

impl cosmic::Application for OverlayApp {
    type Executor = cosmic::executor::Default;
    type Flags = AppMode;
    type Message = Message;

    const APP_ID: &'static str = "io.github.hasmolam.CosmicTranslatorOverlay";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, mode: Self::Flags) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let selector_id = Id::unique();
        let hud_id = Id::unique();
        let subtitle_seq = 1;

        let initial_text = if mode == AppMode::Hud {
            cosmic_translator_overlay::fl!("hud-active")
        } else {
            String::new()
        };

        let selector_program = match &mode {
            AppMode::Selector { preview_path } => SelectorProgram::new(preview_path.as_deref()),
            _ => SelectorProgram::default(),
        };

        let app = Self {
            core,
            mode: mode.clone(),
            hud: SubtitleHudState {
                surface_id: hud_id,
                current_text: initial_text,
                visible: true,
                ..Default::default()
            },
            selector_program,
            selector_id,
            hud_id,
            subtitle_seq,
        };

        let task = match mode {
            AppMode::Selector { .. } => {
                let settings = SctkLayerSurfaceSettings {
                    id: selector_id,
                    layer: Layer::Overlay,
                    keyboard_interactivity: KeyboardInteractivity::Exclusive,
                    input_zone: None, // Accepts mouse drag and keyboard events
                    exclusive_zone: -1,
                    anchor: Anchor::all(),
                    ..Default::default()
                };
                get_layer_surface(settings)
            }
            AppMode::Hud => {
                let mut settings = app.hud.layer_settings();
                settings.id = hud_id;
                let layer_task = get_layer_surface(settings);
                let clear_task =
                    Task::perform(tokio::time::sleep(Duration::from_millis(3000)), move |_| {
                        cosmic::Action::from(Message::ClearSubtitle(subtitle_seq))
                    });
                Task::batch([layer_task, clear_task])
            }
        };

        (app, task)
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Self::Message> {
        if self.mode == AppMode::Hud {
            hud_ipc_subscription()
        } else {
            cosmic::iced::Subscription::none()
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        tracing::debug!("OverlayApp::update received: {:?}", message);
        match message {
            Message::Selector(SelectorMessage::Selected {
                roi,
                surface_width,
                surface_height,
            }) => {
                let line = match SelectorOutput::new(roi, surface_width, surface_height).to_line() {
                    Ok(line) => line,
                    Err(err) => {
                        tracing::error!("Failed to encode selector output: {err}");
                        selector::CANCELLED_LINE.to_string()
                    }
                };
                let _ = writeln!(std::io::stdout(), "{line}");
                let _ = std::io::stdout().flush();
                let _ = destroy_layer_surface::<Message>(self.selector_id);
                Task::perform(
                    async {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        std::process::exit(0);
                    },
                    |_| cosmic::Action::None,
                )
            }

            Message::Selector(SelectorMessage::Cancelled) => {
                let _ = writeln!(std::io::stdout(), "{}", selector::CANCELLED_LINE);
                let _ = std::io::stdout().flush();
                let _ = destroy_layer_surface::<Message>(self.selector_id);
                Task::perform(
                    async {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        std::process::exit(0);
                    },
                    |_| cosmic::Action::None,
                )
            }

            Message::Hud(HudMessage::ShowSubtitle {
                text, duration_ms, ..
            }) => {
                tracing::debug!(
                    "HUD updating text to: '{}' (duration_ms={})",
                    text,
                    duration_ms
                );
                self.subtitle_seq = self.subtitle_seq.wrapping_add(1);
                let seq = self.subtitle_seq;
                self.hud.current_text = text;
                self.hud.visible = true;

                if duration_ms == 0 {
                    // Persistent subtitle mode: stay displayed continuously until next dialogue replaces it
                    Task::none()
                } else {
                    Task::perform(
                        tokio::time::sleep(Duration::from_millis(duration_ms)),
                        move |_| cosmic::Action::from(Message::ClearSubtitle(seq)),
                    )
                }
            }

            Message::Hud(HudMessage::ClearSubtitle) => {
                tracing::debug!("HUD clearing subtitle explicitly");
                self.subtitle_seq = self.subtitle_seq.wrapping_add(1);
                self.hud.current_text.clear();
                Task::none()
            }

            Message::ClearSubtitle(seq) => {
                if seq == self.subtitle_seq {
                    tracing::debug!("HUD timer clearing subtitle for seq {}", seq);
                    self.hud.current_text.clear();
                } else {
                    tracing::debug!(
                        "HUD ignoring stale ClearSubtitle seq {} (current: {})",
                        seq,
                        self.subtitle_seq
                    );
                }
                Task::none()
            }

            Message::Hud(HudMessage::SetStatus { active, text }) => {
                if active {
                    self.hud.status_badge = Some(text);
                } else {
                    self.hud.status_badge = None;
                }
                Task::none()
            }

            Message::Hud(HudMessage::SetHudStatus { status }) => {
                self.hud.apply_status(status.as_ref());
                Task::none()
            }

            Message::Hud(HudMessage::UpdateStyle {
                font_size,
                opacity,
                duration_ms,
                position,
                offset_y,
            }) => {
                tracing::debug!(
                    "HUD style updated: font_size={}, opacity={}, duration_ms={}, position={:?}, offset_y={}",
                    font_size,
                    opacity,
                    duration_ms,
                    position,
                    offset_y
                );
                self.hud.font_size = font_size;
                self.hud.opacity = opacity;
                self.hud.duration_ms = duration_ms;
                self.hud.position = position;
                self.hud.offset_y = offset_y;
                self.reconfigure_hud()
            }

            Message::Hud(HudMessage::SetVisible { visible }) => {
                tracing::debug!("HUD visibility changed: visible={}", visible);
                self.hud.visible = visible;
                Task::none()
            }

            Message::Hud(HudMessage::SetPosition { position, offset_y }) => {
                tracing::debug!(
                    "HUD position updated: {:?}, offset_y={}",
                    position,
                    offset_y
                );
                self.hud.position = position;
                self.hud.offset_y = offset_y;
                self.reconfigure_hud()
            }

            Message::Hud(HudMessage::SetEditMode { edit_mode }) => {
                tracing::info!("HUD edit mode updated via IPC: {}", edit_mode);
                self.hud.edit_mode = edit_mode;
                if edit_mode && self.hud.current_text.is_empty() {
                    self.hud.current_text = cosmic_translator_overlay::fl!("positioning-active");
                }
                self.reconfigure_hud()
            }

            Message::HudUi(HudUiMessage::LockPosition) => {
                tracing::info!(
                    "Locking HUD position at: {:?}, offset={}",
                    self.hud.position,
                    self.hud.offset_y
                );
                self.hud.edit_mode = false;
                if self.hud.current_text == cosmic_translator_overlay::fl!("positioning-active") {
                    self.hud.current_text.clear();
                }
                Task::batch([
                    self.reconfigure_hud(),
                    save_hud_position(self.hud.position, self.hud.offset_y),
                ])
            }

            Message::HudUi(HudUiMessage::SetPresetPos(pos)) => {
                self.hud.position = pos;
                self.reconfigure_hud()
            }

            Message::HudUi(HudUiMessage::Nudge(direction)) => {
                self.hud.nudge(direction);
                self.reconfigure_hud()
            }

            Message::Hud(HudMessage::Quit) => {
                tracing::info!("HUD received Quit command via IPC; exiting cleanly");
                let _ = destroy_layer_surface::<Message>(self.hud_id);
                std::process::exit(0);
            }

            Message::Hud(HudMessage::Ping) => {
                tracing::debug!("HUD received IPC Ping");
                Task::none()
            }

            Message::Close => match self.mode {
                AppMode::Selector { .. } => {
                    let _ = std::io::stdout().flush();
                    std::process::exit(0);
                }
                AppMode::Hud => destroy_layer_surface(self.hud_id),
            },
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        tracing::debug!("OverlayApp::view called (main window)");
        widget::text("").into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        tracing::debug!("OverlayApp::view_window called for id {:?}", id);
        if id == self.selector_id {
            let canvas_widget = cosmic::iced::widget::canvas(self.selector_program.clone())
                .width(Length::Fill)
                .height(Length::Fill);
            Element::from(canvas_widget).map(Message::Selector)
        } else if id == self.hud_id {
            tracing::debug!(
                "Rendering HUD element with text: '{}'",
                self.hud.current_text
            );
            self.hud.view(Message::HudUi)
        } else {
            widget::text("").into()
        }
    }
}

/// Stores the calibrated position in the applet's settings. The applet watches its
/// config, so it adopts the value and resends it on later style updates instead of
/// restoring the old one.
fn save_hud_position(position: HudPosition, offset_y: u32) -> Task<cosmic::Action<Message>> {
    Task::perform(
        async move {
            let result = tokio::task::spawn_blocking(move || {
                cosmic_translator_overlay::hud::save_position(position, offset_y)
            })
            .await;
            match result {
                Ok(Ok(())) => tracing::info!("Saved HUD position {position:?}, offset {offset_y}"),
                Ok(Err(err)) => tracing::warn!("Failed to save HUD position: {err}"),
                Err(err) => tracing::warn!("HUD position save task failed: {err}"),
            }
        },
        |()| cosmic::Action::None,
    )
}

fn main() -> cosmic::iced::Result {
    let component = if env::args().any(|arg| arg == "--selector") {
        "selector"
    } else {
        "hud"
    };
    if let Some(path) = translator_protocol::logging::init(component) {
        tracing::info!("Logging to {}", path.display());
    }
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    cosmic_translator_overlay::i18n::init(&requested_languages);
    let args: Vec<String> = env::args().collect();
    let mode = if args.iter().any(|arg| arg == "--selector") {
        let preview_path = args
            .iter()
            .find(|arg| arg.ends_with(".png") || arg.ends_with(".bmp"))
            .map(std::path::PathBuf::from);
        AppMode::Selector { preview_path }
    } else {
        AppMode::Hud
    };

    if mode == AppMode::Hud {
        let sock_path = default_socket_path();
        if std::os::unix::net::UnixStream::connect(&sock_path).is_ok() {
            tracing::info!(
                "HUD overlay is already running on {:?}. Exiting cleanly.",
                sock_path
            );
            return Ok(());
        }
    }

    cosmic::app::run::<OverlayApp>(cosmic::app::Settings::default().no_main_window(true), mode)
}
