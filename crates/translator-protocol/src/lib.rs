//! Types shared by the `cosmic-ext-translator` processes: the panel applet, the
//! settings window and the overlay (`--hud`, `--selector`).
//!
//! The overlay depends only on this crate, so it links neither the capture, OCR nor
//! translation code. Keep it free of heavy dependencies: `serde` always, `tokio` for the
//! socket behind the `ipc` feature, `tracing-subscriber` behind `logging`.

pub mod hud;
#[cfg(feature = "logging")]
pub mod logging;
pub mod region;
pub mod selector;

pub use hud::{
    default_socket_path, HudMessage, HudPosition, HudStatus, APPLET_CONFIG_ID,
    APPLET_CONFIG_VERSION, DEFAULT_SUBTITLE_DURATION_MS,
};
#[cfg(feature = "ipc")]
pub use hud::{HudIpcClient, HudIpcServer, SendError};
pub use region::{CaptureRegion, Roi, RoiPreset};
pub use selector::SelectorOutput;
