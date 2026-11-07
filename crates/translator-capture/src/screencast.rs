use crate::{CaptureError, Result};
use ashpd::desktop::screencast::{
    CursorMode, OpenPipeWireRemoteOptions, Screencast, SelectSourcesOptions, SourceType,
    StartCastOptions, Stream,
};
use ashpd::desktop::CreateSessionOptions;
use ashpd::desktop::{PersistMode, Session};
use std::os::fd::OwnedFd;

pub struct ScreencastStreamInfo {
    pub node_id: u32,
    pub pipewire_fd: OwnedFd,
    pub restore_token: Option<String>,
    pub session: Session<Screencast>,
}

pub struct ScreencastManager;

impl ScreencastManager {
    /// Requests a screencast session via XDG Screencast portal.
    ///
    /// If an existing `restore_token` fails (e.g. expired or display topology changed),
    /// it automatically falls back to an interactive session request so capture never fails silently.
    pub async fn create_stream(
        restore_token: Option<&str>,
        persist: bool,
    ) -> Result<ScreencastStreamInfo> {
        if let Some(token) = restore_token {
            match Self::try_create_stream(Some(token), persist).await {
                Ok(info) => Ok(info),
                Err(err) => {
                    tracing::warn!(
                        "Screencast session with restore token failed ({err}); retrying with interactive picker"
                    );
                    Self::try_create_stream(None, persist).await
                }
            }
        } else {
            Self::try_create_stream(None, persist).await
        }
    }

    async fn try_create_stream(
        restore_token: Option<&str>,
        persist: bool,
    ) -> Result<ScreencastStreamInfo> {
        let proxy = Screencast::new().await?;
        let session = proxy
            .create_session(CreateSessionOptions::default())
            .await?;

        let persist_mode = if persist {
            PersistMode::ExplicitlyRevoked
        } else {
            PersistMode::DoNot
        };

        let mut select_options = SelectSourcesOptions::default()
            .set_sources(SourceType::Monitor | SourceType::Window)
            .set_cursor_mode(CursorMode::Hidden)
            .set_multiple(false)
            .set_persist_mode(persist_mode);

        if let Some(token) = restore_token {
            select_options = select_options.set_restore_token(token);
        }

        proxy.select_sources(&session, select_options).await?;

        let start_request = proxy
            .start(&session, None, StartCastOptions::default())
            .await?;

        let response = start_request.response()?;
        let stream: &Stream = response
            .streams()
            .first()
            .ok_or(CaptureError::NoStreamsAvailable)?;

        let node_id = stream.pipe_wire_node_id();
        let saved_token = response.restore_token().map(|s| s.to_string());
        let pipewire_fd = proxy
            .open_pipe_wire_remote(&session, OpenPipeWireRemoteOptions::default())
            .await?;

        Ok(ScreencastStreamInfo {
            node_id,
            pipewire_fd,
            restore_token: saved_token,
            session,
        })
    }
}
