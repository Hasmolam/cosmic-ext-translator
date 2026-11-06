//! Logging setup shared by the applet and overlay binaries.
//!
//! Logs go to stderr and to `$XDG_STATE_HOME/cosmic-ext-translator/<component>.log`
//! (fallback `~/.local/state/...`). Processes started from the panel or the app launcher
//! have their stderr discarded, so the file is the only record of failures there.
//!
//! The default level is `info`, which contains no recognised screen text. `RUST_LOG=debug`
//! adds OCR and translation text to the file.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

const APP_DIR: &str = "cosmic-ext-translator";
/// Size at which the log is moved to `<component>.log.1` on the next start.
const ROTATE_BYTES: u64 = 5 * 1024 * 1024;

/// Directory for log files.
#[must_use]
pub fn log_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join(APP_DIR))
}

/// Installs the global subscriber. Returns the log file path when file logging is active.
///
/// File logging is best effort: if the directory or file cannot be created, logging
/// continues on stderr only.
pub fn init(component: &str) -> Option<PathBuf> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr_layer = fmt::layer().with_writer(io::stderr);

    let file = log_dir().and_then(|dir| match open_log_file(&dir, component) {
        Ok(opened) => Some(opened),
        Err(err) => {
            eprintln!("cosmic-ext-translator: file logging disabled: {err}");
            None
        }
    });

    match file {
        Some((path, file)) => {
            let file_layer = fmt::layer().with_ansi(false).with_writer(Mutex::new(file));
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(stderr_layer)
                .with(file_layer)
                .try_init();
            Some(path)
        }
        None => {
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(stderr_layer)
                .try_init();
            None
        }
    }
}

/// Opens `<dir>/<component>.log` for appending, creating `dir` with mode 0700 and the
/// file with mode 0600. A file larger than `ROTATE_BYTES` is renamed to `.log.1` first.
pub fn open_log_file(dir: &Path, component: &str) -> io::Result<(PathBuf, File)> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let path = dir.join(format!("{component}.log"));
    if fs::metadata(&path)
        .map(|m| m.len() > ROTATE_BYTES)
        .unwrap_or(false)
    {
        fs::rename(&path, dir.join(format!("{component}.log.1")))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&path)?;
    Ok((path, file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cet-log-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn creates_private_dir_and_file() {
        let dir = temp_dir("perm").join("nested");
        let (path, mut file) = open_log_file(&dir, "applet").unwrap();
        writeln!(file, "line").unwrap();
        assert_eq!(path, dir.join("applet.log"));
        let dir_mode = fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        let file_mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(dir_mode, 0o700);
        assert_eq!(file_mode, 0o600);
        let _ = fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn rotates_large_file() {
        let dir = temp_dir("rotate");
        fs::create_dir_all(&dir).unwrap();
        let big = vec![b'x'; usize::try_from(ROTATE_BYTES).unwrap() + 1];
        fs::write(dir.join("hud.log"), &big).unwrap();

        let (path, _file) = open_log_file(&dir, "hud").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        assert_eq!(
            fs::metadata(dir.join("hud.log.1")).unwrap().len(),
            big.len() as u64
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
