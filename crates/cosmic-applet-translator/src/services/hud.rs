//! Single entry point for messages to the HUD overlay.

use std::time::Duration;

use cosmic::iced::Task;
use tracing::{debug, warn};
use translator_core::ipc::{HudIpcClient, HudMessage, HudPosition};

use crate::config::Config;
use crate::services::process::overlay_binary_path;

/// Time given to a freshly spawned HUD to bind its socket.
const HUD_STARTUP_DELAY: Duration = Duration::from_millis(200);

/// Sends messages to the HUD and starts it on demand.
///
/// Delivery failures are logged at debug level: the HUD is optional and the applet keeps
/// working without it.
#[derive(Debug, Clone)]
pub struct HudService {
    client: HudIpcClient,
}

impl Default for HudService {
    fn default() -> Self {
        Self {
            client: HudIpcClient::default_client(),
        }
    }
}

impl HudService {
    pub async fn send(&self, message: &HudMessage) {
        if let Err(err) = self.client.send(message).await {
            debug!("HUD message not delivered: {err}");
        }
    }

    /// Fire-and-forget send as an iced task.
    pub fn send_task<M: Send + 'static>(&self, message: HudMessage) -> Task<cosmic::Action<M>> {
        let hud = self.clone();
        Task::perform(async move { hud.send(&message).await }, |()| {
            cosmic::Action::None
        })
    }

    /// Spawns `cosmic-translator-overlay --hud` if no HUD answers on the socket.
    pub async fn ensure_running(&self) {
        if self.client.is_running().await {
            return;
        }
        if let Err(err) = std::process::Command::new(overlay_binary_path())
            .arg("--hud")
            .spawn()
        {
            warn!("Failed to start HUD overlay: {err}");
            return;
        }
        tokio::time::sleep(HUD_STARTUP_DELAY).await;
    }

    /// Starts the HUD if needed and applies the configured style.
    pub async fn ensure_running_with_style(&self, style: HudMessage) {
        self.ensure_running().await;
        self.send(&style).await;
    }

    /// Sends `Quit` from a short-lived thread. Used from `Drop`, where no async runtime
    /// can be assumed.
    pub fn quit_detached(&self) {
        let hud = self.clone();
        let _ = std::thread::spawn(move || {
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime.block_on(hud.send(&HudMessage::Quit)),
                Err(err) => debug!("Cannot build runtime to stop the HUD: {err}"),
            }
        });
    }
}

#[must_use]
pub fn position_from_str(pos: &str) -> HudPosition {
    match pos {
        "top" => HudPosition::Top,
        "center" => HudPosition::Center,
        _ => HudPosition::Bottom,
    }
}

/// `UpdateStyle` message for the HUD settings in `config`.
#[must_use]
pub fn style_message(config: &Config) -> HudMessage {
    HudMessage::UpdateStyle {
        font_size: config.font_size,
        opacity: f32::from(config.hud_opacity) / 100.0,
        duration_ms: config.hud_duration_ms,
        position: position_from_str(&config.hud_position),
        offset_y: config.hud_offset_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_message_uses_config_values() {
        let config = Config {
            font_size: 30,
            hud_opacity: 50,
            hud_duration_ms: 4000,
            hud_position: "top".into(),
            hud_offset_y: 12,
            ..Config::default()
        };
        assert_eq!(
            style_message(&config),
            HudMessage::UpdateStyle {
                font_size: 30,
                opacity: 0.5,
                duration_ms: 4000,
                position: HudPosition::Top,
                offset_y: 12,
            }
        );
    }

    #[test]
    fn unknown_position_falls_back_to_bottom() {
        assert_eq!(position_from_str("center"), HudPosition::Center);
        assert_eq!(position_from_str("nonsense"), HudPosition::Bottom);
    }
}
