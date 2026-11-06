//! Locating and launching the applet's own executables.

use std::io;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, error};

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

/// Starts a long-running child without blocking and reaps it when it exits, so it does
/// not stay behind as a zombie for the lifetime of the applet.
pub fn spawn_detached(command: &mut Command) -> io::Result<()> {
    let mut child = command.spawn()?;
    let pid = child.id();
    std::thread::Builder::new()
        .name("child-reaper".into())
        .spawn(move || match child.wait() {
            Ok(status) => debug!("Child process {pid} exited: {status}"),
            Err(err) => debug!("Waiting for child process {pid} failed: {err}"),
        })?;
    Ok(())
}

/// Starts the standalone settings window as a separate process.
pub fn spawn_settings_window() {
    if let Err(e) = spawn_detached(&mut Command::new(applet_binary_path())) {
        error!("Failed to launch settings application: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_child_is_reaped() {
        let mut command = Command::new("true");
        spawn_detached(&mut command).unwrap();
        // The reaper thread collects the exit status; no zombie remains afterwards.
        std::thread::sleep(std::time::Duration::from_millis(200));
        let me = std::process::id();
        let zombies = std::fs::read_dir("/proc")
            .unwrap()
            .filter_map(Result::ok)
            .filter_map(|e| std::fs::read_to_string(e.path().join("stat")).ok())
            .filter(|stat| {
                let fields: Vec<&str> = stat
                    .rsplit_once(')')
                    .map(|(_, rest)| rest.split_whitespace().collect())
                    .unwrap_or_default();
                fields.first() == Some(&"Z") && fields.get(1) == Some(&me.to_string().as_str())
            })
            .count();
        assert_eq!(zombies, 0);
    }
}
