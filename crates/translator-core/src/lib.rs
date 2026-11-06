pub mod capture;
pub mod error;
pub mod ipc;
pub mod language;
pub mod ocr;
pub mod orchestrator;
pub mod pipeline;
pub mod shortcuts;
pub mod translation;

pub use capture::pipewire_thread::{CapturedFrame, PipewireCaptureHandle, PipewireCommand};
pub use capture::roi::{CaptureRegion, Roi, RoiPreset};
pub use capture::screencast::{ScreencastManager, ScreencastStreamInfo};
pub use capture::CaptureSession;

pub use pipeline::debouncer::{
    DebounceState, FrameDebouncer, SettledFrame, TextDiffAction, TextGate,
};
pub use pipeline::phash::{dhash_256, dhash_64, hamming_distance_256, hamming_distance_64};
pub use pipeline::preprocess::{
    compute_otsu_threshold, preprocess_for_ocr, subtract_blur, PreprocessConfig,
};
pub use pipeline::{spawn_recognition_pipeline, PipelineHandle, RecognizedEvent};

pub use error::{CaptureError, OcrInitError, Result};
pub use ipc::{default_socket_path, HudIpcClient, HudIpcServer, HudMessage, HudPosition};
pub use language::{normalize_source_code, script_for_code, Script, SourceLanguage};
pub use ocr::{
    create_ocr_backend, create_ocr_backend_for_lang, detect_dominant_orientation, filter_furigana,
    is_cjk_char, is_cjk_text, join_cjk_or_latin_tokens, normalize_cjk_text, sort_cjk_reading_order,
    DetectedTextBox, MockOcrBackend, OcrBackend, OcrResult, OcrsBackend, OrientationMode,
    TesseractBackend, TextOrientation,
};
pub use orchestrator::{
    spawn_orchestrator, OrchestratorConfig, OrchestratorEvent, OrchestratorHandle,
};
pub use shortcuts::{GlobalShortcutsManager, ShortcutAction};
pub use translation::{
    parse_speaker_tags, BracketStyle, DeepLTranslationBackend, DialogueTurn, FuzzyLruCache,
    GoogleTranslateBackend, MockTranslationBackend, OllamaTranslationBackend,
    OpenAiTranslationBackend, ParsedDialogueLine, RateLimitManager, TranslationBackend,
    TranslationContext, DEFAULT_CACHE_CAPACITY, DEFAULT_SIMILARITY_THRESHOLD,
};
