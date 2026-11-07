use cosmic::cosmic_config::{self, ConfigSet};
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
use translator_protocol::{HudPosition, HudStatus, APPLET_CONFIG_ID, APPLET_CONFIG_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudUiMessage {
    LockPosition,
    SetPresetPos(HudPosition),
    Nudge(NudgeDirection),
}

/// On-screen direction of a calibration step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NudgeDirection {
    Up,
    Down,
}

/// Writes a calibrated position to the applet's settings in one transaction, so the
/// applet sees position and offset change together. Blocking file I/O.
pub fn save_position(position: HudPosition, offset_y: u32) -> Result<(), cosmic_config::Error> {
    let config = cosmic_config::Config::new(APPLET_CONFIG_ID, APPLET_CONFIG_VERSION)?;
    let tx = config.transaction();
    tx.set("hud_position", position.as_config_str())?;
    tx.set("hud_offset_y", offset_y)?;
    tx.commit()
}

/// Distance moved by one calibration step, in logical pixels.
pub const NUDGE_STEP: u32 = 20;
/// Upper bound for the edge offset.
pub const MAX_OFFSET_Y: u32 = 400;

#[derive(Debug, Clone)]
pub struct SubtitleHudState {
    pub surface_id: Id,
    pub current_text: String,
    pub status_badge: Option<String>,
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
            status_badge: None,
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

/// Localised badge text for a pipeline status.
#[must_use]
pub fn status_label(status: &HudStatus) -> String {
    match status {
        HudStatus::RateLimited { retry_after_secs } => {
            let secs: u64 = *retry_after_secs;
            crate::fl!("status-rate-limited", secs = secs)
        }
        HudStatus::TranslationFailed => crate::fl!("status-translation-failed"),
        HudStatus::OcrUnavailable { reason } => {
            let reason: String = reason.clone();
            crate::fl!("status-ocr-unavailable", reason = reason)
        }
    }
}

impl SubtitleHudState {
    /// Moves the HUD one step up or down on screen.
    ///
    /// `offset_y` is the margin from the anchored edge, so its sign depends on the
    /// anchor: at the bottom edge moving up grows the margin, at the top edge it shrinks
    /// it. A centered HUD has no anchored vertical edge and does not move.
    pub fn nudge(&mut self, direction: NudgeDirection) {
        let grow = match (self.position, direction) {
            (HudPosition::Bottom, NudgeDirection::Up)
            | (HudPosition::Top, NudgeDirection::Down) => true,
            (HudPosition::Bottom, NudgeDirection::Down)
            | (HudPosition::Top, NudgeDirection::Up) => false,
            (HudPosition::Center, _) => return,
        };
        self.offset_y = if grow {
            self.offset_y.saturating_add(NUDGE_STEP).min(MAX_OFFSET_Y)
        } else {
            self.offset_y.saturating_sub(NUDGE_STEP)
        };
    }

    /// Shows or clears the status badge. Does not touch the subtitle text.
    pub fn apply_status(&mut self, status: Option<&HudStatus>) {
        self.status_badge = status.map(status_label);
    }

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
            None // Accept clicks and button interactions in interactive calibration mode
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
        let has_text = !self.current_text.trim().is_empty();
        let has_badge = self
            .status_badge
            .as_ref()
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

        if !self.visible || (!has_text && !has_badge && !self.edit_mode) {
            return widget::container(widget::text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
        }

        let display_text = if self.current_text.trim().is_empty() {
            crate::fl!("positioning-active")
        } else {
            self.current_text.clone()
        };

        let label = widget::text(display_text)
            .size(self.font_size)
            .wrapping(cosmic::iced::widget::text::Wrapping::Word);
        let opacity = self.opacity.clamp(0.1, 1.0);

        if self.edit_mode {
            let edit_title =
                widget::text(crate::fl!("positioning-title", offset = self.offset_y)).size(13);

            let lock_btn = widget::button::suggested(crate::fl!("lock-position"))
                .on_press(map_ui(HudUiMessage::LockPosition));
            let top_btn = widget::button::standard(crate::fl!("pos-top"))
                .on_press(map_ui(HudUiMessage::SetPresetPos(HudPosition::Top)));
            let center_btn = widget::button::standard(crate::fl!("pos-center"))
                .on_press(map_ui(HudUiMessage::SetPresetPos(HudPosition::Center)));
            let btm_btn = widget::button::standard(crate::fl!("pos-bottom"))
                .on_press(map_ui(HudUiMessage::SetPresetPos(HudPosition::Bottom)));
            let mut controls = widget::row![top_btn, center_btn, btm_btn]
                .spacing(8)
                .align_y(Alignment::Center);
            // A centered HUD has no edge offset to adjust.
            if self.position != HudPosition::Center {
                let up_btn = widget::button::standard(crate::fl!("nudge-up"))
                    .on_press(map_ui(HudUiMessage::Nudge(NudgeDirection::Up)));
                let down_btn = widget::button::standard(crate::fl!("nudge-down"))
                    .on_press(map_ui(HudUiMessage::Nudge(NudgeDirection::Down)));
                controls = controls.push(up_btn).push(down_btn);
            }
            let controls = controls.push(lock_btn);

            let edit_pill = widget::container(
                widget::column![edit_title, label, controls]
                    .spacing(8)
                    .align_x(Alignment::Center),
            )
            .padding(Padding::from([12, 20]))
            .style(move |_theme| widget::container::Style {
                background: Some(Background::Color(Color::from_rgba(0.04, 0.04, 0.08, 0.95))),
                border: Border {
                    color: Color::from_rgba(0.9, 0.6, 0.2, 0.95), // Amber border in calibration mode
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

        let badge_widget_opt = if let Some(ref badge_text) = self.status_badge {
            if !badge_text.trim().is_empty() {
                let badge_label = widget::text(badge_text.as_str())
                    .size(13)
                    .wrapping(cosmic::iced::widget::text::Wrapping::Word);

                let badge_pill = widget::container(badge_label)
                    .padding(Padding::from([6, 16]))
                    .style(|_theme| widget::container::Style {
                        background: Some(Background::Color(Color::from_rgba(
                            0.90, 0.45, 0.08, 0.95,
                        ))),
                        border: Border {
                            color: Color::from_rgba(1.0, 0.75, 0.25, 0.95),
                            width: 1.5,
                            radius: 8.0.into(),
                        },
                        ..Default::default()
                    });
                Some(badge_pill)
            } else {
                None
            }
        } else {
            None
        };

        let pill_container_opt = if has_text {
            let pill = widget::container(label)
                .padding(Padding::from([14, 28]))
                .max_width(1200.0)
                .style(move |_theme| widget::container::Style {
                    background: Some(Background::Color(Color::from_rgba(
                        0.04, 0.04, 0.06, opacity,
                    ))),
                    border: Border {
                        color: Color::from_rgba(1.0, 1.0, 1.0, 0.35),
                        width: 2.0,
                        radius: 14.0.into(),
                    },
                    ..Default::default()
                });
            Some(pill)
        } else {
            None
        };

        let content: Element<'_, Message> = match (badge_widget_opt, pill_container_opt) {
            (Some(badge), Some(pill)) => widget::column![badge, pill]
                .spacing(8)
                .align_x(Alignment::Center)
                .into(),
            (Some(badge), None) => badge.into(),
            (None, Some(pill)) => pill.into(),
            (None, None) => widget::text("").into(),
        };

        widget::container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hud(position: HudPosition, offset_y: u32) -> SubtitleHudState {
        SubtitleHudState {
            position,
            offset_y,
            ..Default::default()
        }
    }

    #[test]
    fn nudge_up_moves_bottom_hud_away_from_bottom_edge() {
        let mut state = hud(HudPosition::Bottom, 40);
        state.nudge(NudgeDirection::Up);
        assert_eq!(state.offset_y, 60);
        state.nudge(NudgeDirection::Down);
        state.nudge(NudgeDirection::Down);
        assert_eq!(state.offset_y, 20);
    }

    #[test]
    fn nudge_up_moves_top_hud_towards_top_edge() {
        let mut state = hud(HudPosition::Top, 40);
        state.nudge(NudgeDirection::Up);
        assert_eq!(state.offset_y, 20);
        state.nudge(NudgeDirection::Down);
        assert_eq!(state.offset_y, 40);
    }

    #[test]
    fn nudge_is_clamped() {
        let mut state = hud(HudPosition::Top, 10);
        state.nudge(NudgeDirection::Up);
        assert_eq!(state.offset_y, 0);
        let mut state = hud(HudPosition::Bottom, MAX_OFFSET_Y - 5);
        state.nudge(NudgeDirection::Up);
        assert_eq!(state.offset_y, MAX_OFFSET_Y);
    }

    #[test]
    fn centered_hud_ignores_nudge() {
        let mut state = hud(HudPosition::Center, 40);
        state.nudge(NudgeDirection::Up);
        state.nudge(NudgeDirection::Down);
        assert_eq!(state.offset_y, 40);
    }
}
