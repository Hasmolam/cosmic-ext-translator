//! Standalone settings window: status header, capture controls, live inspection and
//! the settings pages.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, button, row};
use cosmic::Element;
use image::RgbaImage;

use super::{AppModel, Message};
use crate::fl;
use crate::preview::PreviewImages;

/// Image shown in the live inspection card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreviewMode {
    #[default]
    RawCropped,
    Preprocessed,
}

impl AppModel {
    pub(super) fn update_previews(&mut self, image: &RgbaImage) {
        let config = super::translation::preprocess_config(&self.config);
        if let Some(images) = crate::preview::build(image, &config) {
            self.set_previews(images);
        }
    }

    pub(super) fn set_previews(&mut self, images: PreviewImages) {
        self.last_raw_preview = Some(images.raw);
        self.last_proc_preview = Some(images.preprocessed);
    }

    pub(super) fn view_dashboard(&self) -> Element<'_, Message> {
        let header = row![
            widget::text("cosmic-ext-translator")
                .size(20)
                .wrapping(cosmic::iced::widget::text::Wrapping::Word),
            cosmic::iced::widget::space::horizontal(),
            widget::text(format!("{}: {}", fl!("status"), self.capture_status_text())).size(14),
            widget::toggler(self.capture_active).on_toggle(Message::ToggleCapture),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let quick_actions = row![
            button::suggested(fl!("select-target")).on_press(Message::ChangeCaptureTarget),
            button::standard(fl!("select-roi")).on_press(Message::SelectRoiRequested),
            button::standard(fl!("translate-now")).on_press(Message::OneShotTranslate),
            widget::text(format!("{}: {}", fl!("region"), self.region_description())).size(13),
        ]
        .spacing(10)
        .align_y(Alignment::Center);

        let mut top_section = widget::column![header].spacing(10);
        if let Some(error) = self.capture_error_view() {
            top_section = top_section.push(
                widget::container(error)
                    .padding(10)
                    .width(Length::Fill)
                    .class(cosmic::theme::Container::Card),
            );
        }
        let top_section = top_section.push(
            widget::column![
                widget::divider::horizontal::default(),
                quick_actions,
                widget::divider::horizontal::default(),
                self.inspection_card(),
                widget::divider::horizontal::default(),
            ]
            .spacing(10),
        );

        let settings = self
            .settings_state
            .view(&self.config, &self.backlog)
            .map(Message::Settings);

        widget::container(
            widget::column![top_section, settings]
                .spacing(10)
                .padding(16),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .class(cosmic::theme::Container::WindowBackground)
        .into()
    }

    /// Cropped capture region (raw or pre-processed) with the last OCR and translation.
    fn inspection_card(&self) -> Element<'_, Message> {
        let preview_handle = match self.preview_mode {
            PreviewMode::RawCropped => self.last_raw_preview.as_ref(),
            PreviewMode::Preprocessed => self.last_proc_preview.as_ref(),
        };
        let preview: Element<'_, Message> = if let Some(handle) = preview_handle {
            widget::container(
                widget::image(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fixed(110.0)),
            )
            .width(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        } else {
            widget::container(widget::text(fl!("prompt-select-target")).size(13))
                .padding(14)
                .center_x(Length::Fill)
                .into()
        };

        let mode_button = |mode: PreviewMode, label: String| {
            if self.preview_mode == mode {
                button::suggested(label)
            } else {
                button::standard(label)
            }
            .on_press(Message::TogglePreviewMode(mode))
        };
        let preview_selector = row![
            widget::text(fl!("live-inspection-title")).size(13),
            cosmic::iced::widget::space::horizontal(),
            mode_button(PreviewMode::RawCropped, fl!("raw-crop")),
            mode_button(PreviewMode::Preprocessed, fl!("ocr-binarize")),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let ocr_feedback = widget::column![
            feedback_row(fl!("last-detected-ocr"), &self.last_ocr_text),
            feedback_row(fl!("translation-output"), &self.last_translated_text),
        ]
        .spacing(4);

        widget::container(
            widget::column![
                preview_selector,
                widget::divider::horizontal::default(),
                preview,
                widget::divider::horizontal::default(),
                ocr_feedback,
            ]
            .spacing(8)
            .padding(10),
        )
        .class(cosmic::theme::Container::Card)
        .into()
    }
}

fn feedback_row<'a>(label: String, value: &'a str) -> Element<'a, Message> {
    row![
        widget::text(format!("{label}: ")).size(13),
        widget::text(if value.is_empty() { "-" } else { value })
            .size(13)
            .wrapping(cosmic::iced::widget::text::Wrapping::Word),
    ]
    .into()
}
