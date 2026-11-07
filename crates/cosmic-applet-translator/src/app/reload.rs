//! Reload of the panel applet after an install.
//!
//! cosmic-panel restarts an applet only when it exits with a non-zero code; an applet
//! killed by a signal stays stopped until the panel itself restarts. Restarting the
//! whole panel several times in a row makes cosmic-session delay the next start by
//! minutes and can leave the panel without its notifications connection.
//!
//! Instead, `packaging/reload-applet.sh` sends SIGHUP. The applet stops capture and
//! exits with [`RELOAD_EXIT_CODE`], and the panel starts the newly installed binary.

use cosmic::iced::futures::channel::mpsc::Sender;
use tokio::signal::unix::{signal, SignalKind};
use tracing::{info, warn};

use super::{AppModel, Message};

/// Exit code that makes cosmic-panel start the applet again (`EX_TEMPFAIL`).
pub const RELOAD_EXIT_CODE: i32 = 75;

impl AppModel {
    pub(super) fn reload(&mut self) -> ! {
        info!("Reload requested, exiting with code {RELOAD_EXIT_CODE} so the panel restarts the applet");
        self.stop_capture_session();
        std::process::exit(RELOAD_EXIT_CODE)
    }
}

/// Emits `ReloadRequested` on SIGHUP.
pub(super) fn subscription() -> cosmic::iced::Subscription<Message> {
    struct ReloadSub;

    cosmic::iced::Subscription::run_with(std::any::TypeId::of::<ReloadSub>(), |_| {
        cosmic::iced::stream::channel(1, |mut output: Sender<Message>| async move {
            use cosmic::iced::futures::SinkExt;
            match signal(SignalKind::hangup()) {
                Ok(mut hangup) => {
                    if hangup.recv().await.is_some() {
                        let _ = output.send(Message::ReloadRequested).await;
                    }
                }
                Err(err) => warn!("SIGHUP handler not installed, reload unavailable: {err}"),
            }
            cosmic::iced::futures::future::pending::<()>().await;
        })
    })
}
