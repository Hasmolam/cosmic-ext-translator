//! Locating and launching the applet's own executables.

use std::path::PathBuf;
use tracing::error;

const OVERLAY_BIN: &str = "cosmic-translator-overlay";
const APPLET_BIN: &str = "cosmic-applet-translator";

/// Overlay executable: next to the running binary, then `target/debug` for development
/// runs, then `$PATH`.
#[must_use]
pub fn overlay_binary_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join(OVERLAY_BIN);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    let target_debug = PathBuf::from("target/debug").join(OVERLAY_BIN);
    if target_debug.exists() {
        return target_debug;
    }
    PathBuf::from(OVERLAY_BIN)
}

/// Applet executable, used to open the standalone settings window.
#[must_use]
pub fn applet_binary_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if exe.is_file() {
            return exe;
        }
    }
    PathBuf::from(APPLET_BIN)
}

/// Starts the standalone settings window as a separate process.
pub fn spawn_settings_window() {
    if let Err(e) = std::process::Command::new(applet_binary_path()).spawn() {
        error!("Failed to launch settings application: {e}");
    }
}
