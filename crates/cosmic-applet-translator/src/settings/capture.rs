//! Capture page: region preset, custom and ignore regions, OCR pre-processing.

use cosmic::iced::Alignment;
use cosmic::widget::{self, button, row, text};
use cosmic::Element;
use translator_protocol::{Roi, RoiPreset};

use super::{choice_button, with_value_label, Request, Update};
use crate::config::Config;
use crate::fl;
use crate::services::RegionKind;

#[derive(Debug, Clone)]
pub enum Message {
    PresetSelected(RoiPreset),
    SelectCustomRegion,
    SelectIgnoreRegion,
    ClearIgnoreRegion,
    BinarizeToggled(bool),
    BlurSubtractionToggled(bool),
    SettleDurationChanged(u64),
}

pub fn update(config: &mut Config, message: Message) -> Update<Message> {
    match message {
        Message::PresetSelected(preset) => config.preset = preset.as_str().to_string(),
        Message::SelectCustomRegion => {
            return Update::request(Request::SelectRegion(RegionKind::Capture))
        }
        Message::SelectIgnoreRegion => {
            return Update::request(Request::SelectRegion(RegionKind::Ignore))
        }
        Message::ClearIgnoreRegion => config.ignore_roi = None,
        Message::BinarizeToggled(value) => config.binarize = value,
        Message::BlurSubtractionToggled(value) => config.blur_subtraction = value,
        Message::SettleDurationChanged(ms) => config.settle_duration_ms = ms,
    }
    Update::none()
}

/// Localised long label of a preset.
#[must_use]
pub fn preset_label(preset: RoiPreset) -> String {
    match preset {
        RoiPreset::Bottom25 => fl!("preset-bottom25-full"),
        RoiPreset::Bottom33 => fl!("preset-bottom33-full"),
        RoiPreset::FullScreen => fl!("preset-fullscreen-full"),
        RoiPreset::Custom => fl!("preset-custom-full"),
    }
}

/// `WxH @ (x, y)`.
#[must_use]
pub fn format_roi(roi: Roi) -> String {
    format!("{}x{} @ ({}, {})", roi.width, roi.height, roi.x, roi.y)
}

pub fn view(config: &Config) -> Element<'_, Message> {
    let current_preset = RoiPreset::from_str_name(&config.preset);

    let mut preset_row = row![].spacing(6);
    for preset in RoiPreset::ALL {
        preset_row = preset_row.push(choice_button(
            preset_label(preset),
            preset == current_preset,
            Message::PresetSelected(preset),
        ));
    }

    let custom_coords = config
        .custom_roi
        .map_or_else(|| fl!("no-custom-area"), format_roi);
    let ignore_coords = config
        .ignore_roi
        .map_or_else(|| fl!("no-ignore-area"), format_roi);

    let mut ignore_row = row![
        text(ignore_coords),
        button::standard(fl!("draw-ignore-area")).on_press(Message::SelectIgnoreRegion),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    if config.ignore_roi.is_some() {
        ignore_row = ignore_row.push(
            button::destructive(fl!("clear-ignore-area")).on_press(Message::ClearIgnoreRegion),
        );
    }

    widget::settings::section()
        .title(fl!("section-capture-title"))
        .add(widget::settings::item(
            fl!("preset-dialog-template"),
            preset_row,
        ))
        .add(widget::settings::item(
            fl!("custom-coords"),
            row![
                text(custom_coords),
                button::standard(fl!("draw-new-area")).on_press(Message::SelectCustomRegion),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ))
        .add(widget::settings::item(fl!("ignore-roi"), ignore_row))
        .add(widget::settings::item(
            fl!("otsu-binarization"),
            widget::toggler(config.binarize).on_toggle(Message::BinarizeToggled),
        ))
        .add(widget::settings::item(
            fl!("blur-subtraction"),
            widget::toggler(config.blur_subtraction).on_toggle(Message::BlurSubtractionToggled),
        ))
        .add(widget::settings::item(
            fl!("settle-threshold"),
            with_value_label(
                widget::slider(100..=1000, settle_slider_value(config), |val| {
                    Message::SettleDurationChanged(u64::from(val))
                })
                .width(180),
                format!("{} ms", config.settle_duration_ms),
            ),
        ))
        .into()
}

fn settle_slider_value(config: &Config) -> u32 {
    u32::try_from(config.settle_duration_ms).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_buttons_request_the_selector() {
        let mut config = Config::default();
        let update = update(&mut config, Message::SelectIgnoreRegion);
        assert_eq!(
            update.request,
            Some(Request::SelectRegion(RegionKind::Ignore))
        );
        assert_eq!(config, Config::default());
    }

    #[test]
    fn clear_ignore_region_edits_config() {
        let mut config = Config {
            ignore_roi: Some(Roi::new(1, 2, 3, 4)),
            ..Config::default()
        };
        let update = update(&mut config, Message::ClearIgnoreRegion);
        assert!(update.request.is_none());
        assert_eq!(config.ignore_roi, None);
    }
}
