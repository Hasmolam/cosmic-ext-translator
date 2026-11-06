//! Applies config changes to the running capture, OCR, orchestrator and HUD.
//!
//! Every config change, whether made in this process or picked up from `cosmic-config`
//! (settings window, HUD calibration), goes through [`AppModel::apply_config_change`].
//! The effects are derived from the difference between the old and new config, so a
//! change written by this process and echoed back by the config watcher does nothing.

use cosmic::iced::Task;
use tracing::info;

use super::ocr::ocr_settings_differ;
use super::translation::translation_settings_differ;
use super::{AppModel, Message};
use crate::config::Config;
use crate::services::hud::style_message;

/// What has to be refreshed after a config change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfigDelta {
    /// Capture preset, custom region or ignore region.
    pub region: bool,
    /// Source language, text direction or OCR engine.
    pub ocr: bool,
    /// Inputs of the translation orchestrator.
    pub translation: bool,
    /// Anything sent to the HUD in `UpdateStyle`.
    pub hud_style: bool,
}

impl ConfigDelta {
    #[must_use]
    pub fn between(old: &Config, new: &Config) -> Self {
        Self {
            region: old.preset != new.preset
                || old.custom_roi != new.custom_roi
                || old.ignore_roi != new.ignore_roi,
            ocr: ocr_settings_differ(old, new),
            translation: translation_settings_differ(old, new),
            hud_style: style_message(old) != style_message(new),
        }
    }
}

impl AppModel {
    /// Changes the config, saves it and applies the effects.
    pub(super) fn edit_config(
        &mut self,
        edit: impl FnOnce(&mut Config),
    ) -> Task<cosmic::Action<Message>> {
        let old = self.config.clone();
        edit(&mut self.config);
        if self.config == old {
            return Task::none();
        }
        self.save_config();
        self.apply_config_change(&old)
    }

    /// Applies the difference between `old` and the current config.
    pub(super) fn apply_config_change(&mut self, old: &Config) -> Task<cosmic::Action<Message>> {
        let delta = ConfigDelta::between(old, &self.config);

        if delta.region {
            self.apply_capture_region();
        }

        let pipeline_task = if delta.ocr && self.capture_session.is_some() {
            info!(
                "OCR settings changed (source '{}', orientation '{}', engine '{}'), rebuilding OCR backend",
                self.config.source_lang, self.config.ocr_orientation, self.config.ocr_engine
            );
            // The orchestrator is restarted when the new backend is loaded.
            self.rebuild_ocr_backend()
        } else {
            if delta.translation {
                self.restart_orchestrator_if_active();
            }
            Task::none()
        };

        let hud_task = if delta.hud_style {
            self.hud.send_task(style_message(&self.config))
        } else {
            Task::none()
        };

        Task::batch([pipeline_task, hud_task])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use translator_core::Roi;

    #[test]
    fn identical_configs_have_no_effect() {
        let config = Config::default();
        assert_eq!(
            ConfigDelta::between(&config, &config.clone()),
            ConfigDelta::default()
        );
    }

    #[test]
    fn restore_token_has_no_effect() {
        let old = Config::default();
        let new = Config {
            restore_token: Some("token".into()),
            ..Config::default()
        };
        assert_eq!(ConfigDelta::between(&old, &new), ConfigDelta::default());
    }

    #[test]
    fn each_group_is_detected() {
        let old = Config::default();

        let region = Config {
            ignore_roi: Some(Roi::new(0, 0, 10, 10)),
            ..Config::default()
        };
        assert_eq!(
            ConfigDelta::between(&old, &region),
            ConfigDelta {
                region: true,
                ..ConfigDelta::default()
            }
        );

        let style = Config {
            hud_offset_y: old.hud_offset_y + 20,
            ..Config::default()
        };
        assert_eq!(
            ConfigDelta::between(&old, &style),
            ConfigDelta {
                hud_style: true,
                ..ConfigDelta::default()
            }
        );

        let source = Config {
            source_lang: "ja".into(),
            ..Config::default()
        };
        let delta = ConfigDelta::between(&old, &source);
        assert!(delta.ocr && delta.translation && !delta.region && !delta.hud_style);

        // The orchestrator hides subtitles after this duration, and the HUD gets it too.
        let duration = Config {
            hud_duration_ms: old.hud_duration_ms + 1000,
            ..Config::default()
        };
        let delta = ConfigDelta::between(&old, &duration);
        assert!(delta.translation && delta.hud_style && !delta.ocr);
    }
}
