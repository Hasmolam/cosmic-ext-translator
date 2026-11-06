//! Panel popup: capture toggle, presets and shortcuts to the settings window.

use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{window::Id, Length, Limits, Task};
use cosmic::widget::{self, button, row};
use cosmic::Element;
use translator_core::region::RoiPreset;

use super::ocr::OcrStatus;
use super::{AppModel, Message};
use crate::fl;

impl AppModel {
    pub(super) fn toggle_popup(&mut self) -> Task<cosmic::Action<Message>> {
        if let Some(popup) = self.popup.take() {
            return destroy_popup(popup);
        }
        let Some(main_id) = self.core.main_window_id() else {
            return Task::none();
        };
        let new_id = Id::unique();
        self.popup = Some(new_id);
        let mut popup_settings = self
            .core
            .applet
            .get_popup_settings(main_id, new_id, None, None, None);
        popup_settings.positioner.size_limits = Limits::NONE
            .max_width(400.0)
            .min_width(320.0)
            .min_height(240.0)
            .max_height(1080.0);
        get_popup(popup_settings)
    }

    pub(super) fn close_popup(&mut self) -> Task<cosmic::Action<Message>> {
        match self.popup.take() {
            Some(popup) => destroy_popup(popup),
            None => Task::none(),
        }
    }

    pub(super) fn view_popup(&self) -> Element<'_, Message> {
        let current_preset = RoiPreset::from_str_name(&self.config.preset);
        let preset_button = |preset: RoiPreset, label: String| {
            if current_preset == preset {
                button::suggested(label)
            } else {
                button::standard(label)
            }
            .width(Length::Fill)
            .on_press(Message::PresetSelected(preset))
        };

        let preset_controls = widget::column![
            widget::text(fl!("preset")).size(14),
            row![
                preset_button(RoiPreset::Bottom25, fl!("preset-bottom25-short")),
                preset_button(RoiPreset::Bottom33, fl!("preset-bottom33-short")),
            ]
            .spacing(6)
            .width(Length::Fill),
            row![preset_button(
                RoiPreset::FullScreen,
                fl!("preset-fullscreen-short")
            )]
            .width(Length::Fill),
        ]
        .spacing(6);

        let mut content = widget::list_column()
            .add(widget::settings::item(
                fl!("status"),
                widget::text(self.capture_status_text()),
            ))
            .add(widget::settings::item(
                fl!("enable-translation"),
                widget::toggler(self.capture_active).on_toggle(Message::ToggleCapture),
            ));

        match &self.ocr_status {
            OcrStatus::Loading => {
                content = content.add(widget::settings::item(
                    fl!("ocr-status"),
                    widget::text(fl!("ocr-loading")),
                ));
            }
            OcrStatus::Unavailable(reason) => {
                let detail = widget::column![
                    widget::text(fl!("ocr-unavailable")).size(14),
                    widget::text(reason.as_str()).size(12),
                ]
                .spacing(2);
                content = content.add(widget::settings::item_row(vec![detail.into()]));
            }
            OcrStatus::Idle | OcrStatus::Ready => {}
        }
        if let Some(error) = self.capture_error_view() {
            content = content.add(widget::settings::item_row(vec![error]));
        }

        let content = content
            .add(preset_controls)
            .add(widget::settings::item(
                fl!("active-region"),
                widget::text(self.region_description()),
            ))
            .add(button::suggested(fl!("select-target")).on_press(Message::ChangeCaptureTarget))
            .add(button::standard(fl!("select-roi")).on_press(Message::SelectRoiRequested))
            .add(button::standard(fl!("translate-now")).on_press(Message::OneShotTranslate))
            .add(button::suggested(fl!("settings")).on_press(Message::OpenSettings));

        self.core
            .applet
            .popup_container(widget::container(content).width(380))
            .into()
    }
}
