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
use translator_core::ipc::HudPosition;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudUiMessage {
    LockPosition,
    SetPresetPos(HudPosition),
    NudgeOffset(i32),
}

#[derive(Debug, Clone)]
pub struct SubtitleHudState {
    pub surface_id: Id,
    pub current_text: String,
    pub visible: bool,
    pub font_size: u16,
    pub opacity: f32,
    pub duration_ms: u64,
    pub position: HudPosition,
    pub offset_y: u32,
    pub edit_mode: bool,
}

impl Default for SubtitleHudState {
    fn default() -> Self {
        Self {
            surface_id: Id::unique(),
            current_text: String::new(),
            visible: true,
            font_size: 22,
            opacity: 0.92,
            duration_ms: 0, // 0 = persistent until next dialogue text arrives
            position: HudPosition::Bottom,
            offset_y: 40,
            edit_mode: false,
        }
    }
}

impl SubtitleHudState {
    #[must_use]
    pub fn layer_settings(&self) -> SctkLayerSurfaceSettings {
        let anchor = match self.position {
            HudPosition::Top => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            HudPosition::Bottom => Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            HudPosition::Center => Anchor::LEFT | Anchor::RIGHT,
        };

        let margin = match self.position {
            HudPosition::Top => IcedMargin {
                top: self.offset_y as i32,
                bottom: 0,
                left: 60,
                right: 60,
            },
            HudPosition::Bottom => IcedMargin {
                bottom: self.offset_y as i32,
                top: 0,
                left: 60,
                right: 60,
            },
            HudPosition::Center => IcedMargin {
                top: 0,
                bottom: 0,
                left: 60,
                right: 60,
            },
        };

        let input_zone = if self.edit_mode {
            None // Accept clicks and button interactions in MORT edit mode
        } else {
            Some(vec![]) // 100% click-through to games in normal mode
        };

        SctkLayerSurfaceSettings {
            id: self.surface_id,
            layer: Layer::Overlay,
            keyboard_interactivity: if self.edit_mode {
                KeyboardInteractivity::OnDemand
            } else {
                KeyboardInteractivity::None
            },
            input_zone,
            exclusive_zone: -1,
            anchor,
            margin,
            size: Some((None, Some(if self.edit_mode { 220 } else { 180 }))),
            ..Default::default()
        }
    }

    pub fn view<F, Message: 'static + Clone>(&self, map_ui: F) -> Element<'_, Message>
    where
        F: Fn(HudUiMessage) -> Message + Copy + 'static,
    {
        if !self.visible || (self.current_text.trim().is_empty() && !self.edit_mode) {
            return widget::container(widget::text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        let display_text = if self.current_text.trim().is_empty() {
            "🎮 Test Altyazı: MORT Konumlandırma Modu Aktif"
        } else {
            &self.current_text
        };

        let label = widget::text(display_text)
            .size(self.font_size)
            .wrapping(cosmic::iced::widget::text::Wrapping::Word);
        let opacity = self.opacity.clamp(0.1, 1.0);

        if self.edit_mode {
            let edit_title = widget::text(format!(
                "✥ MORT Konumlandırma Modu (Dikey Ofset: {} px)",
                self.offset_y
            ))
            .size(13);

            let lock_btn = widget::button::suggested("🔒 Sabitle (Oyuna Dön)")
                .on_press(map_ui(HudUiMessage::LockPosition));
            let top_btn = widget::button::standard("Üst")
                .on_press(map_ui(HudUiMessage::SetPresetPos(HudPosition::Top)));
            let center_btn = widget::button::standard("Orta")
                .on_press(map_ui(HudUiMessage::SetPresetPos(HudPosition::Center)));
            let btm_btn = widget::button::standard("Alt")
                .on_press(map_ui(HudUiMessage::SetPresetPos(HudPosition::Bottom)));
            let dec_btn = widget::button::standard("▲ Yukarı (-20px)")
                .on_press(map_ui(HudUiMessage::NudgeOffset(-20)));
            let inc_btn = widget::button::standard("▼ Aşağı (+20px)")
                .on_press(map_ui(HudUiMessage::NudgeOffset(20)));

            let controls = widget::row![top_btn, center_btn, btm_btn, dec_btn, inc_btn, lock_btn]
                .spacing(8)
                .align_y(Alignment::Center);

            let edit_pill = widget::container(
                widget::column![edit_title, label, controls]
                    .spacing(8)
                    .align_x(Alignment::Center),
            )
            .padding(Padding::from([12, 20]))
            .style(move |_theme| widget::container::Style {
                background: Some(Background::Color(Color::from_rgba(0.04, 0.04, 0.08, 0.95))),
                border: Border {
                    color: Color::from_rgba(0.9, 0.6, 0.2, 0.95), // Glowing amber border in MORT mode
                    width: 3.0,
                    radius: 14.0.into(),
                },
                ..Default::default()
            });

            return widget::container(edit_pill)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into();
        }

        let pill_container = widget::container(label)
            .padding(Padding::from([14, 28]))
            .max_width(1200.0)
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
