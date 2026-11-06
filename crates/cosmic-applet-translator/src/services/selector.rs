//! Runs the overlay's interactive region selector and parses its result.

use tracing::{debug, warn};
use translator_core::capture::pipewire_thread::SnapshotRequester;
use translator_core::ipc::SelectorOutput;

use crate::services::process::overlay_binary_path;

/// Which configured region a selection is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// The capture region that is OCR'd.
    Capture,
    /// An area inside the capture region that is masked out (portraits, animations).
    Ignore,
}

impl RegionKind {
    fn preview_file_tag(self) -> &'static str {
        match self {
            Self::Capture => "full",
            Self::Ignore => "ignore",
        }
    }
}

/// Shows the selector, using a full-frame snapshot of the active capture as backdrop
/// when one is available. Returns `None` when the user cancels or the selector fails.
pub async fn select_region(
    kind: RegionKind,
    snapshot: Option<SnapshotRequester>,
) -> Option<SelectorOutput> {
    let backdrop = match snapshot {
        Some(requester) => requester.get_full_snapshot().await,
        None => None,
    };

    let preview_path = backdrop.and_then(|img| {
        let path = std::env::temp_dir().join(format!(
            "cosmic-ext-translator-{}-{}.bmp",
            kind.preview_file_tag(),
            std::process::id()
        ));
        match img.save_with_format(&path, image::ImageFormat::Bmp) {
            Ok(()) => Some(path),
            Err(err) => {
                debug!("Selector backdrop not written: {err}");
                None
            }
        }
    });

    let mut cmd = tokio::process::Command::new(overlay_binary_path());
    cmd.arg("--selector");
    if let Some(path) = &preview_path {
        cmd.arg(path);
    }
    let output = cmd.output().await;
    if let Some(path) = &preview_path {
        let _ = std::fs::remove_file(path);
    }

    match output {
        Ok(out) => SelectorOutput::parse_stdout(&String::from_utf8_lossy(&out.stdout)),
        Err(err) => {
            warn!("Failed to run region selector: {err}");
            None
        }
    }
}
