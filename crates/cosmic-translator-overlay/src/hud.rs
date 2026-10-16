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
    pub font_size: u16,
    pub opacity: f32,
    pub duration_ms: u64,
}

impl Default for SubtitleHudState {
    fn default() -> Self {
        Self {
            surface_id: Id::unique(),
            current_text: String::new(),
            visible: true,
            font_size: 22,
            opacity: 0.92,
            duration_ms: 6000,
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
            anchor: Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            margin: IcedMargin {
                bottom: 40,
                top: 0,
                left: 0,
                right: 0,
            },
            size: Some((None, Some(110))),
            ..Default::default()
        }
    }

    pub fn view<Message: 'static>(&self) -> Element<'_, Message> {
        if !self.visible || self.current_text.trim().is_empty() {
            // Transparent empty container filling surface
            return widget::container(widget::text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        let label = widget::text(&self.current_text).size(self.font_size);
        let opacity = self.opacity.clamp(0.1, 1.0);

        let pill_container = widget::container(label)
            .padding(Padding::from([14, 28]))
            .style(move |_theme| widget::container::Style {
                background: Some(Background::Color(Color::from_rgba(0.04, 0.04, 0.06, opacity))),
                border: Border {
                    color: Color::from_rgba(1.0, 1.0, 1.0, 0.35),
                    width: 2.0,
                    radius: 14.0.into(),
                },
                ..Default::default()
            });

        widget::container(pill_container)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }
}
