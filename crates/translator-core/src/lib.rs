pub mod error;
pub mod capture;
pub mod pipeline;
pub mod ocr;
pub mod translation;
pub mod shortcuts;
pub mod ipc;
pub mod orchestrator;

pub use capture::roi::{CaptureRegion, Roi, RoiPreset};
pub use capture::screencast::{ScreencastManager, ScreencastStreamInfo};
pub use capture::pipewire_thread::{CapturedFrame, PipewireCaptureHandle, PipewireCommand};
pub use capture::CaptureSession;

pub use pipeline::phash::{dhash_64, dhash_256, hamming_distance_64, hamming_distance_256};
pub use pipeline::debouncer::{DebounceState, FrameDebouncer, SettledFrame};
pub use pipeline::preprocess::{preprocess_for_ocr, compute_otsu_threshold, PreprocessConfig};
pub use pipeline::{spawn_recognition_pipeline, PipelineHandle, RecognizedEvent};

pub use ocr::{MockOcrBackend, OcrBackend, OcrsBackend};
pub use translation::{
    FuzzyLruCache, MockTranslationBackend, OllamaTranslationBackend, TranslationBackend,
    TranslationContext, DEFAULT_CACHE_CAPACITY, DEFAULT_SIMILARITY_THRESHOLD,
};
pub use shortcuts::{GlobalShortcutsManager, ShortcutAction};
pub use ipc::{default_socket_path, HudIpcClient, HudIpcServer, HudMessage};
pub use orchestrator::{
    spawn_orchestrator, OrchestratorConfig, OrchestratorEvent, OrchestratorHandle,
};
pub use error::{CaptureError, Result};
