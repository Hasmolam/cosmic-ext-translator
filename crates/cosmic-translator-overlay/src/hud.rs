use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{
    IcedMargin, SctkLayerSurfaceSettings,
};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    Anchor, KeyboardInteractivity, Layer,
};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Background, Border, Color, Length, Padding};
use cosmic::widget;
use cosmic::Element;

#[derive(Debug, Clone)]
pub struct SubtitleHudState {
    pub surface_id: Id,
    pub current_text: String,
    pub visible: bool,
}

impl Default for SubtitleHudState {
    fn default() -> Self {
        Self {
            surface_id: Id::unique(),
            current_text: String::new(),
            visible: true,
        }
    }
}

impl SubtitleHudState {
    #[must_use]
    pub fn layer_settings(&self) -> SctkLayerSurfaceSettings {
        SctkLayerSurfaceSettings {
            id: self.surface_id,
            layer: Layer::Overlay,
            keyboard_interactivity: KeyboardInteractivity::None,
            // Empty input zone ensures 100% click-through (all clicks pass to underlying game/window)
            input_zone: Some(vec![]),
            exclusive_zone: -1,
            anchor: Anchor::BOTTOM,
            margin: IcedMargin {
                bottom: 80,
                top: 0,
                left: 0,
                right: 0,
            },
            size: None, // Auto-sizes to text content
            ..Default::default()
        }
    }

    pub fn view<Message: 'static>(&self) -> Element<'_, Message> {
        if !self.visible || self.current_text.trim().is_empty() {
            // Invisible when there is no text
            return widget::text("").into();
        }

        let label = widget::text(&self.current_text).size(22);

        let pill_container = widget::container(label)
            .padding(Padding::from([12, 24]))
            .style(|_theme| widget::container::Style {
                background: Some(Background::Color(Color::from_rgba(0.04, 0.04, 0.06, 0.88))),
                border: Border {
                    color: Color::from_rgba(1.0, 1.0, 1.0, 0.25),
                    width: 1.5,
                    radius: 12.0.into(),
                },
                ..Default::default()
            });

        widget::container(pill_container)
            .width(Length::Shrink)
            .align_x(Alignment::Center)
            .into()
    }
}
