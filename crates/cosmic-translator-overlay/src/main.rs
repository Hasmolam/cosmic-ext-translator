mod hud;
mod selector;

use std::env;
use std::time::Duration;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::SctkLayerSurfaceSettings;
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, get_layer_surface, Anchor, KeyboardInteractivity, Layer,
};
use cosmic::iced::window::Id;
use cosmic::iced::{Length, Task};
use cosmic::widget;
use cosmic::Element;
use hud::SubtitleHudState;
use selector::{SelectorMessage, SelectorProgram};
use tokio_stream::wrappers::ReceiverStream;
use translator_core::ipc::{HudIpcServer, HudMessage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Hud,
    Selector,
}

pub struct OverlayApp {
    core: cosmic::Core,
    mode: AppMode,
    hud: SubtitleHudState,
    selector_id: Id,
    hud_id: Id,
}

#[derive(Debug, Clone)]
pub enum Message {
    Selector(SelectorMessage),
    Hud(HudMessage),
    ClearSubtitle,
    Close,
}

fn hud_ipc_subscription() -> cosmic::iced::Subscription<Message> {
    cosmic::iced::Subscription::run(|| {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        tokio::spawn(async move {
            let server = HudIpcServer::default_server();
            if let Ok(mut server_rx) = server.start().await {
                while let Some(msg) = server_rx.recv().await {
                    if tx.send(Message::Hud(msg)).await.is_err() {
                        break;
                    }
                }
            }
        });
        ReceiverStream::new(rx)
    })
}

impl cosmic::Application for OverlayApp {
    type Executor = cosmic::executor::Default;
    type Flags = AppMode;
    type Message = Message;

    const APP_ID: &'static str = "com.system76.CosmicTranslatorOverlay";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, mode: Self::Flags) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let selector_id = Id::unique();
        let hud_id = Id::unique();

        let initial_text = if mode == AppMode::Hud {
            "🎮 cosmic-ext-lingua: Altyazı Katmanı Aktif".to_string()
        } else {
            String::new()
        };

        let app = Self {
            core,
            mode,
            hud: SubtitleHudState {
                surface_id: hud_id,
                current_text: initial_text,
                visible: true,
                ..Default::default()
            },
            selector_id,
            hud_id,
        };

        let task = match mode {
            AppMode::Selector => {
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
                let clear_task = Task::perform(
                    tokio::time::sleep(Duration::from_millis(30000)),
                    |_| cosmic::Action::from(Message::ClearSubtitle),
                );
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
        eprintln!("OverlayApp::update received: {:?}", message);
        match message {
            Message::Selector(SelectorMessage::Selected(roi)) => {
                println!(
                    "ROI_SELECTED:{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
                    roi.x, roi.y, roi.width, roi.height
                );
                destroy_layer_surface(self.selector_id)
            }

            Message::Selector(SelectorMessage::Cancelled) => {
                println!("ROI_CANCELLED");
                destroy_layer_surface(self.selector_id)
            }

            Message::Hud(HudMessage::ShowSubtitle { text, duration_ms, .. }) => {
                eprintln!("HUD updating text to: '{}'", text);
                self.hud.current_text = text;
                self.hud.visible = true;
                let duration = if duration_ms == 0 { self.hud.duration_ms } else { duration_ms };
                Task::perform(
                    tokio::time::sleep(Duration::from_millis(duration)),
                    |_| cosmic::Action::from(Message::ClearSubtitle),
                )
            }

            Message::Hud(HudMessage::ClearSubtitle) | Message::ClearSubtitle => {
                eprintln!("HUD clearing subtitle");
                self.hud.current_text.clear();
                Task::none()
            }

            Message::Hud(HudMessage::SetStatus { active, text }) => {
                if !active {
                    self.hud.current_text.clear();
                } else {
                    self.hud.current_text = text;
                }
                Task::none()
            }

            Message::Hud(HudMessage::UpdateStyle { font_size, opacity, duration_ms }) => {
                eprintln!("HUD style updated: font_size={}, opacity={}, duration_ms={}", font_size, opacity, duration_ms);
                self.hud.font_size = font_size;
                self.hud.opacity = opacity;
                self.hud.duration_ms = duration_ms;
                Task::none()
            }

            Message::Close => match self.mode {
                AppMode::Selector => destroy_layer_surface(self.selector_id),
                AppMode::Hud => destroy_layer_surface(self.hud_id),
            },
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        eprintln!("OverlayApp::view called (main window)");
        widget::text("").into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        eprintln!("OverlayApp::view_window called for id {:?}", id);
        if id == self.selector_id {
            let canvas_widget = cosmic::iced::widget::canvas(SelectorProgram)
                .width(Length::Fill)
                .height(Length::Fill);
            Element::from(canvas_widget).map(Message::Selector)
        } else if id == self.hud_id {
            eprintln!("Rendering HUD element with text: '{}'", self.hud.current_text);
            self.hud.view()
        } else {
            widget::text("").into()
        }
    }
}

fn main() -> cosmic::iced::Result {
    let _ = tracing_subscriber::fmt::try_init();
    let mode = if env::args().any(|arg| arg == "--selector") {
        AppMode::Selector
    } else {
        AppMode::Hud
    };

    cosmic::app::run::<OverlayApp>(
        cosmic::app::Settings::default(),
        mode,
    )
}
