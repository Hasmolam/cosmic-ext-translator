pub mod selector;

pub use selector::SelectorOutput;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Default display duration for subtitles in milliseconds (6 seconds).
pub const DEFAULT_SUBTITLE_DURATION_MS: u64 = 6000;

/// Default socket path resolution.
pub fn default_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("cosmic-ext-translator-hud.sock")
    } else {
        std::env::temp_dir().join("cosmic-ext-translator-hud.sock")
    }
}

/// `cosmic-config` namespace of the applet settings. The HUD writes its calibrated
/// position there so the applet and later sessions pick it up.
pub const APPLET_CONFIG_ID: &str = "io.github.hasmolam.CosmicExtTranslator";
/// Must match `#[version]` on the applet's `Config`.
pub const APPLET_CONFIG_VERSION: u64 = 1;

/// Subtitle HUD vertical screen position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HudPosition {
    Top,
    #[default]
    Bottom,
    Center,
}

impl HudPosition {
    /// Value stored in the applet's `hud_position` setting.
    #[must_use]
    pub const fn as_config_str(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Center => "center",
        }
    }

    /// Parses the applet's `hud_position` setting; unknown values mean `Bottom`.
    #[must_use]
    pub fn from_config_str(value: &str) -> Self {
        match value {
            "top" => Self::Top,
            "center" => Self::Center,
            _ => Self::Bottom,
        }
    }
}

/// Pipeline condition shown as a badge above the subtitle.
///
/// `translator-core` has no localisation; the overlay maps each variant to a Fluent string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HudStatus {
    /// Translation engine returned HTTP 429; requests resume after `retry_after_secs`.
    RateLimited { retry_after_secs: u64 },
    /// Translation request failed for a reason other than rate limiting.
    TranslationFailed,
    /// No OCR backend could be created for the configured source language.
    OcrUnavailable { reason: String },
}

/// Message payload communicated between the translation engine and the HUD overlay.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum HudMessage {
    ShowSubtitle {
        text: String,
        original: Option<String>,
        #[serde(default = "default_duration")]
        duration_ms: u64,
    },
    ClearSubtitle,
    /// Free-form status badge. Kept for compatibility; new code sends `SetHudStatus`.
    SetStatus {
        active: bool,
        text: String,
    },
    /// Typed status badge, localised by the overlay. `None` clears the badge.
    SetHudStatus {
        status: Option<HudStatus>,
    },
    UpdateStyle {
        font_size: u16,
        opacity: f32,
        duration_ms: u64,
        #[serde(default)]
        position: HudPosition,
        #[serde(default = "default_offset_y")]
        offset_y: u32,
    },
    SetPosition {
        position: HudPosition,
        #[serde(default = "default_offset_y")]
        offset_y: u32,
    },
    SetVisible {
        visible: bool,
    },
    SetEditMode {
        edit_mode: bool,
    },
    Quit,
    Ping,
}

fn default_offset_y() -> u32 {
    40
}

fn default_duration() -> u64 {
    0 // 0 means persistent / stay until next subtitle arrives
}

/// Server running in the HUD overlay, receiving messages from the translation pipeline.
pub struct HudIpcServer {
    socket_path: PathBuf,
}

impl HudIpcServer {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: path.as_ref().to_path_buf(),
        }
    }

    pub fn default_server() -> Self {
        Self::new(default_socket_path())
    }

    /// Starts the IPC server, returning a channel of incoming `HudMessage`s.
    pub async fn start(&self) -> std::io::Result<mpsc::Receiver<HudMessage>> {
        // Clean up stale socket file if it exists
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        info!("HUD IPC server listening on {:?}", self.socket_path);

        let (tx, rx) = mpsc::channel(32);
        let path = self.socket_path.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        let tx_clone = tx.clone();
                        tokio::spawn(async move {
                            let (reader, _) = stream.into_split();
                            let mut lines = BufReader::new(reader).lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                if line.trim().is_empty() {
                                    continue;
                                }
                                match serde_json::from_str::<HudMessage>(&line) {
                                    Ok(msg) => {
                                        debug!("HUD IPC received message: {:?}", msg);
                                        if tx_clone.send(msg).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(err) => {
                                        warn!("Malformed HUD IPC payload: {err} | Raw: {line}");
                                    }
                                }
                            }
                        });
                    }
                    Err(err) => {
                        error!("HUD IPC accept error: {err}");
                        break;
                    }
                }
            }
            let _ = std::fs::remove_file(path);
        });

        Ok(rx)
    }
}

/// Client used by the translator applet/orchestrator to dispatch messages to the HUD.
#[derive(Debug, Clone)]
pub struct HudIpcClient {
    socket_path: PathBuf,
}

impl HudIpcClient {
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: path.as_ref().to_path_buf(),
        }
    }

    pub fn default_client() -> Self {
        Self::new(default_socket_path())
    }

    /// Sends a `HudMessage` to the HUD overlay.
    pub async fn send(&self, message: &HudMessage) -> anyhow::Result<()> {
        let mut stream = UnixStream::connect(&self.socket_path).await?;
        let mut payload = serde_json::to_vec(message)?;
        payload.push(b'\n');
        stream.write_all(&payload).await?;
        stream.flush().await?;
        Ok(())
    }

    /// Checks if a HUD overlay instance is actively listening on the socket.
    pub async fn is_running(&self) -> bool {
        if !self.socket_path.exists() {
            return false;
        }
        match UnixStream::connect(&self.socket_path).await {
            Ok(mut stream) => {
                let msg = HudMessage::Ping;
                if let Ok(mut payload) = serde_json::to_vec(&msg) {
                    payload.push(b'\n');
                    if stream.write_all(&payload).await.is_ok() && stream.flush().await.is_ok() {
                        return true;
                    }
                }
                false
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ipc_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let sock_path = temp_dir.join(format!("test_ipc_{}.sock", std::process::id()));

        let server = HudIpcServer::new(&sock_path);
        let mut rx = server.start().await.expect("Failed to bind server");

        let client = HudIpcClient::new(&sock_path);
        assert!(client.is_running().await);
        let ping_msg = rx.recv().await.expect("Failed to receive Ping");
        assert_eq!(ping_msg, HudMessage::Ping);

        let msg = HudMessage::ShowSubtitle {
            text: "Merhaba Dünya!".to_string(),
            original: Some("Hello World!".to_string()),
            duration_ms: 5000,
        };

        client.send(&msg).await.expect("Failed to send message");

        let received = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
            .await
            .expect("Timed out waiting for message")
            .expect("Channel closed");

        assert_eq!(received, msg);

        let _ = std::fs::remove_file(sock_path);
    }

    #[test]
    fn test_hud_status_wire_format() {
        let msg = HudMessage::SetHudStatus {
            status: Some(HudStatus::RateLimited {
                retry_after_secs: 30,
            }),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert_eq!(
            json,
            r#"{"type":"SetHudStatus","payload":{"status":{"kind":"rate_limited","retry_after_secs":30}}}"#
        );
        assert_eq!(serde_json::from_str::<HudMessage>(&json).unwrap(), msg);

        let clear = HudMessage::SetHudStatus { status: None };
        let json = serde_json::to_string(&clear).unwrap();
        assert_eq!(serde_json::from_str::<HudMessage>(&json).unwrap(), clear);
    }

    #[tokio::test]
    async fn test_ipc_is_not_running_when_no_server() {
        let temp_dir = std::env::temp_dir();
        let sock_path = temp_dir.join(format!("test_ipc_dead_{}.sock", std::process::id()));
        let client = HudIpcClient::new(&sock_path);
        assert!(!client.is_running().await);
    }
}
