pub mod roi;
pub mod screencast;
pub mod pipewire_thread;

use roi::Roi;
use screencast::{ScreencastManager, ScreencastStreamInfo};
use pipewire_thread::{spawn_pipewire_capture_thread, CapturedFrame, PipewireCaptureHandle};
use crate::error::Result;

pub struct CaptureSession {
    pub stream_info: ScreencastStreamInfo,
    pub capture_handle: PipewireCaptureHandle,
    pub frame_receiver: tokio::sync::watch::Receiver<Option<CapturedFrame>>,
}

impl CaptureSession {
    /// Starts a complete capture session:
    /// 1. Prompts for screen/window via XDG Portal (or reuses restore token)
    /// 2. Spawns dedicated PipeWire thread
    /// 3. Returns frame receiver channel and session handle
    pub async fn start(
        restore_token: Option<&str>,
        persist: bool,
        initial_roi: Option<Roi>,
    ) -> Result<Self> {
        let stream_info = ScreencastManager::create_stream(restore_token, persist).await?;
        let (capture_handle, frame_receiver) = spawn_pipewire_capture_thread(
            stream_info.pipewire_fd.try_clone().map_err(|e| crate::error::CaptureError::PipeWire(e.to_string()))?,
            stream_info.node_id,
            initial_roi,
        );

        Ok(Self {
            stream_info,
            capture_handle,
            frame_receiver,
        })
    }

    pub fn update_roi(&self, roi: Option<Roi>) {
        self.capture_handle.update_roi(roi);
    }

    pub fn stop(&mut self) -> Result<()> {
        self.capture_handle.stop()
    }
}
