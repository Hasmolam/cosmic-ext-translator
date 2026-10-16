pub mod error;
pub mod capture;

pub use capture::roi::Roi;
pub use capture::screencast::{ScreencastManager, ScreencastStreamInfo};
pub use capture::pipewire_thread::{CapturedFrame, PipewireCaptureHandle, PipewireCommand};
pub use capture::CaptureSession;
pub use error::{CaptureError, Result};
