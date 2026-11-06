//! Capture session lifecycle, region selection and one-shot translation.

use cosmic::iced::Task;
use tracing::{error, info, warn};
use translator_core::ipc::{HudMessage, SelectorOutput};
use translator_core::pipeline::preprocess::preprocess_for_ocr;
use translator_core::region::{CaptureRegion, RoiPreset};
use translator_core::translation::TranslationContext;
use translator_core::{normalize_source_code, CaptureSession, Roi};

use super::translation::{preprocess_config, translation_backend};
use super::{AppModel, Message};
use crate::config::Config;
use crate::services::hud::style_message;
use crate::services::{self, RegionKind};

/// Capture region described by the config.
#[must_use]
pub fn capture_region(config: &Config) -> CaptureRegion {
    CaptureRegion::new(
        RoiPreset::from_str_name(&config.preset),
        config.custom_roi,
        config.ignore_roi,
    )
}

impl AppModel {
    /// Asks the portal for a screencast and starts the HUD. The result arrives as
    /// `CaptureStarted` or `CaptureFailed`.
    pub(super) fn start_capture(&mut self) -> Task<cosmic::Action<Message>> {
        self.capture_active = true;
        self.capture_starting = true;
        self.capture_error = None;

        let hud = self.hud.clone();
        let style = style_message(&self.config);
        let restore_token = self.config.restore_token.clone();
        let region = capture_region(&self.config);

        Task::perform(
            async move {
                hud.ensure_running_with_style(style).await;
                match CaptureSession::start(restore_token.as_deref(), true, region).await {
                    Ok(session) => Message::CaptureStarted(session),
                    Err(err) => Message::CaptureFailed {
                        error: err.to_string(),
                        cancelled: err.is_cancelled(),
                    },
                }
            },
            cosmic::Action::from,
        )
    }

    pub(super) fn capture_started(
        &mut self,
        session: CaptureSession,
    ) -> Task<cosmic::Action<Message>> {
        self.capture_starting = false;
        info!("Capture session active, loading OCR backend");
        if session.stream_info.restore_token != self.config.restore_token {
            self.config.restore_token = session.stream_info.restore_token.clone();
            self.save_config();
        }

        if let Some(frame) = session.frame_receiver.borrow().as_ref() {
            self.update_previews(&frame.image);
        }

        self.capture_generation = self.capture_generation.wrapping_add(1);
        self.capture_session = Some(session);
        self.rebuild_ocr_backend()
    }

    pub(super) fn capture_failed(&mut self, error: String, cancelled: bool) {
        self.capture_active = false;
        self.capture_starting = false;
        if cancelled {
            info!("Screen sharing dialog was cancelled");
        } else {
            error!("Capture start failed: {error}");
            self.capture_error = Some(error);
        }
        if self.config.restore_token.is_some() {
            warn!("Resetting invalid restore token after capture failure to prevent lockouts");
            self.config.restore_token = None;
            self.save_config();
        }
    }

    /// Stops the orchestrator and the capture session.
    pub(super) fn stop_capture_session(&mut self) {
        self.stop_orchestrator();
        if let Some(mut session) = self.capture_session.take() {
            if let Err(err) = session.stop() {
                warn!("Capture session did not stop cleanly: {err}");
            }
        }
    }

    pub(super) fn stop_capture(&mut self) -> Task<cosmic::Action<Message>> {
        self.capture_active = false;
        self.capture_starting = false;
        self.stop_capture_session();
        self.reset_ocr_backend();
        let hud = self.hud.clone();
        Task::future(async move {
            hud.send(&HudMessage::ClearSubtitle).await;
            hud.send(&HudMessage::SetHudStatus { status: None }).await;
        })
        .discard()
    }

    /// Forgets the portal restore token and asks for a new window or screen.
    pub(super) fn change_capture_target(&mut self) -> Task<cosmic::Action<Message>> {
        info!("User requested selecting a new target window or screen");
        self.config.restore_token = None;
        self.save_config();
        self.stop_capture_session();
        self.start_capture()
    }

    /// Sends the configured region to the capture thread.
    pub(super) fn apply_capture_region(&self) {
        if let Some(session) = &self.capture_session {
            session.update_region(capture_region(&self.config));
        }
    }

    pub(super) fn run_region_selector(&self, kind: RegionKind) -> Task<cosmic::Action<Message>> {
        let snapshot = self
            .capture_session
            .as_ref()
            .map(CaptureSession::snapshot_requester);
        Task::perform(
            services::selector::select_region(kind, snapshot),
            move |output| cosmic::Action::from(Message::RegionSelectorFinished(kind, output)),
        )
    }

    pub(super) fn region_selected(
        &mut self,
        kind: RegionKind,
        output: Option<SelectorOutput>,
    ) -> Task<cosmic::Action<Message>> {
        let Some(output) = output else {
            info!("{kind:?} region selection was cancelled");
            return Task::none();
        };
        let roi = self.selection_in_frame_coordinates(output);
        info!("Selected {kind:?} region: {roi:?}");
        self.edit_config(|config| match kind {
            RegionKind::Capture => {
                config.custom_roi = Some(roi);
                config.preset = RoiPreset::Custom.as_str().to_string();
            }
            RegionKind::Ignore => config.ignore_roi = Some(roi),
        })
    }

    /// Scales a selection from the selector's logical surface to the capture buffer.
    /// Falls back to the unscaled region when either size is unknown.
    fn selection_in_frame_coordinates(&self, output: SelectorOutput) -> Roi {
        let roi = output.roi();
        let (Some(sw), Some(sh), Some(session)) = (
            output.surface_width,
            output.surface_height,
            &self.capture_session,
        ) else {
            return roi;
        };
        let Some((fw, fh)) = session
            .frame_receiver
            .borrow()
            .as_ref()
            .map(|f| (f.original_width, f.original_height))
        else {
            return roi;
        };
        let scaled = roi.scale_to_frame(sw, sh, fw, fh);
        info!("Scaled selection from surface ({sw}x{sh}) to frame ({fw}x{fh}): {scaled:?}");
        scaled
    }

    /// OCR and translation of the current frame, outside the orchestrator's debouncing.
    /// Starts capture instead when no session is running.
    pub(super) fn one_shot_translate(&mut self) -> Task<cosmic::Action<Message>> {
        let Some(session) = &self.capture_session else {
            info!("One-shot translation requested but capture is inactive, starting capture");
            return self.start_capture();
        };
        let Some(frame) = session.frame_receiver.borrow().clone() else {
            info!("No frame available yet for one-shot translate");
            return Task::none();
        };
        self.update_previews(&frame.image);

        let Some(ocr) = self.ocr_backend.clone() else {
            return Task::none();
        };
        let backend = translation_backend(&self.config);
        let preprocess = preprocess_config(&self.config);
        let src_lang = normalize_source_code(&self.config.source_lang);
        let target_lang = self.config.target_lang.clone();
        let glossary = self.config.glossary.clone();
        let duration_ms = self.config.hud_duration_ms;
        let hud = self.hud.clone();

        Task::perform(
            async move {
                let gray = preprocess_for_ocr(&frame.image, &preprocess);
                let text = match ocr.recognize(&image::DynamicImage::ImageLuma8(gray)).await {
                    Ok(text) => text.trim().to_string(),
                    Err(err) => {
                        warn!("One-shot OCR failed: {err}");
                        return Message::OneShotCompleted(None);
                    }
                };
                if text.is_empty() {
                    return Message::OneShotCompleted(None);
                }

                let mut ctx = TranslationContext::default();
                for (term, translation) in glossary {
                    ctx.set_glossary_term(term, translation);
                }
                match backend
                    .translate(&text, &src_lang, &target_lang, &ctx)
                    .await
                {
                    Ok(translated) => {
                        hud.send(&HudMessage::ShowSubtitle {
                            text: translated.clone(),
                            original: Some(text.clone()),
                            duration_ms,
                        })
                        .await;
                        Message::OneShotCompleted(Some((text, translated)))
                    }
                    Err(err) => {
                        warn!("One-shot translation failed: {err}");
                        Message::OneShotCompleted(None)
                    }
                }
            },
            cosmic::Action::from,
        )
    }
}
