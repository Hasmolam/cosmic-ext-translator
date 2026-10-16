mod hud;
mod selector;

use std::env;
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
    UpdateSubtitle(String),
    Close,
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

        let app = Self {
            core,
            mode,
            hud: SubtitleHudState {
                surface_id: hud_id,
                current_text: "Translation Overlay Active".into(),
                visible: true,
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
                get_layer_surface(settings)
            }
        };

        (app, task)
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
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

            Message::UpdateSubtitle(text) => {
                self.hud.current_text = text;
                Task::none()
            }

            Message::Close => match self.mode {
                AppMode::Selector => destroy_layer_surface(self.selector_id),
                AppMode::Hud => destroy_layer_surface(self.hud_id),
            },
        }
    }

    fn view(&self) -> Element<'_, Self::Message> {
        widget::text("").into()
    }

    fn view_window(&self, id: Id) -> Element<'_, Self::Message> {
        if id == self.selector_id {
            let canvas_widget = cosmic::iced::widget::canvas(SelectorProgram)
                .width(Length::Fill)
                .height(Length::Fill);
            Element::from(canvas_widget).map(Message::Selector)
        } else if id == self.hud_id {
            self.hud.view()
        } else {
            widget::text("").into()
        }
    }
}

fn main() -> cosmic::iced::Result {
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
