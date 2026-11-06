//! Display page: HUD position, size, opacity and visibility duration.

use cosmic::iced::Task;
use cosmic::widget::{self, button, row};
use cosmic::Element;
use translator_core::ipc::HudMessage;

use super::{choice_button, with_value_label, Update};
use crate::config::Config;
use crate::fl;
use crate::services::hud::style_message;
use crate::services::HudService;

/// Display duration used when switching from persistent to timed subtitles.
const DEFAULT_DURATION_MS: u64 = 6000;

#[derive(Debug, Clone)]
pub enum Message {
    PositionChanged(String),
    PersistentToggled(bool),
    OffsetYChanged(u32),
    FontSizeChanged(u16),
    OpacityChanged(u8),
    DurationChanged(u64),
    TestHudRequested,
    StartInteractivePositioning,
}

pub fn update(config: &mut Config, hud: &HudService, message: Message) -> Update<Message> {
    match message {
        Message::PositionChanged(position) => config.hud_position = position,
        Message::PersistentToggled(persistent) => {
            config.hud_persistent = persistent;
            if persistent {
                config.hud_duration_ms = 0;
            } else if config.hud_duration_ms == 0 {
                config.hud_duration_ms = DEFAULT_DURATION_MS;
            }
        }
        Message::OffsetYChanged(offset) => config.hud_offset_y = offset,
        Message::FontSizeChanged(size) => config.font_size = size,
        Message::OpacityChanged(opacity) => config.hud_opacity = opacity,
        Message::DurationChanged(ms) => config.hud_duration_ms = ms,
        Message::TestHudRequested => {
            let hud = hud.clone();
            let style = style_message(config);
            let duration_ms = config.hud_duration_ms;
            let subtitle = HudMessage::ShowSubtitle {
                text: fl!("test-subtitle-content"),
                original: Some("Test Subtitle".to_string()),
                duration_ms,
            };
            return Update::task(
                Task::future(async move {
                    hud.ensure_running_with_style(style).await;
                    hud.send(&subtitle).await;
                })
                .discard(),
            );
        }
        Message::StartInteractivePositioning => {
            let hud = hud.clone();
            let style = style_message(config);
            return Update::task(
                Task::future(async move {
                    hud.ensure_running_with_style(style).await;
                    hud.send(&HudMessage::SetEditMode { edit_mode: true }).await;
                })
                .discard(),
            );
        }
    }
    Update::none()
}

pub fn view(config: &Config) -> Element<'_, Message> {
    let current = match config.hud_position.as_str() {
        "top" => "top",
        "center" => "center",
        _ => "bottom",
    };
    let mut position_row = row![].spacing(6);
    for (code, label) in [
        ("bottom", fl!("pos-bottom")),
        ("top", fl!("pos-top")),
        ("center", fl!("pos-center")),
    ] {
        position_row = position_row.push(choice_button(
            label,
            current == code,
            Message::PositionChanged(code.to_string()),
        ));
    }

    let mut section = widget::settings::section()
        .title(fl!("hud-layer-title"))
        .add(widget::settings::item(fl!("hud-position"), position_row))
        .add(widget::settings::item(
            fl!("hud-vertical-offset"),
            with_value_label(
                widget::slider(0..=300, config.hud_offset_y, Message::OffsetYChanged).width(180),
                format!("{} px", config.hud_offset_y),
            ),
        ))
        .add(widget::settings::item(
            fl!("font-size"),
            with_value_label(
                widget::slider(16..=36, config.font_size, Message::FontSizeChanged).width(180),
                format!("{} px", config.font_size),
            ),
        ))
        .add(widget::settings::item(
            fl!("background-opacity"),
            with_value_label(
                widget::slider(20..=100, config.hud_opacity, Message::OpacityChanged).width(180),
                format!("{} %", config.hud_opacity),
            ),
        ))
        .add(widget::settings::item(
            fl!("hud-persistent"),
            widget::toggler(config.hud_persistent).on_toggle(Message::PersistentToggled),
        ));

    if !config.hud_persistent {
        let duration = if config.hud_duration_ms == 0 {
            DEFAULT_DURATION_MS
        } else {
            config.hud_duration_ms
        };
        let slider_value = u32::try_from(duration).unwrap_or(u32::MAX);
        section = section.add(widget::settings::item(
            fl!("hud-auto-hide-time"),
            with_value_label(
                widget::slider(2000..=15000, slider_value, |val| {
                    Message::DurationChanged(u64::from(val))
                })
                .width(180),
                fl!(
                    "duration-seconds",
                    sec = format!("{:.1}", duration as f32 / 1000.0)
                ),
            ),
        ));
    }

    section
        .add(widget::settings::item(
            fl!("interactive-positioning"),
            button::suggested(fl!("positioning-button"))
                .on_press(Message::StartInteractivePositioning),
        ))
        .add(widget::settings::item(
            fl!("live-preview"),
            button::standard(fl!("show-test-hud-message")).on_press(Message::TestHudRequested),
        ))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistent_toggle_sets_and_restores_duration() {
        let hud = HudService::default();
        let mut config = Config {
            hud_duration_ms: 0,
            hud_persistent: true,
            ..Config::default()
        };
        let _ = update(&mut config, &hud, Message::PersistentToggled(false));
        assert_eq!(config.hud_duration_ms, DEFAULT_DURATION_MS);

        let _ = update(&mut config, &hud, Message::PersistentToggled(true));
        assert!(config.hud_persistent);
        assert_eq!(config.hud_duration_ms, 0);
    }
}
