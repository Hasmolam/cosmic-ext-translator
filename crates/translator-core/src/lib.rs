pub mod error;
pub mod capture;
pub mod pipeline;
pub mod ocr;
pub mod translation;
pub mod dictionary;
pub mod shortcuts;
pub mod ipc;
pub mod orchestrator;

pub use capture::roi::{CaptureRegion, Roi, RoiPreset};
pub use capture::screencast::{ScreencastManager, ScreencastStreamInfo};
pub use capture::pipewire_thread::{CapturedFrame, PipewireCaptureHandle, PipewireCommand};
pub use capture::CaptureSession;

pub use pipeline::phash::{dhash_64, dhash_256, hamming_distance_64, hamming_distance_256};
pub use pipeline::debouncer::{DebounceState, FrameDebouncer, SettledFrame, TextDiffAction, TextGate};
pub use pipeline::preprocess::{preprocess_for_ocr, compute_otsu_threshold, subtract_blur, PreprocessConfig};
pub use pipeline::{spawn_recognition_pipeline, PipelineHandle, RecognizedEvent};

pub use ocr::{
    create_ocr_backend_for_lang, detect_dominant_orientation, filter_furigana, is_cjk_char,
    is_cjk_text, join_cjk_or_latin_tokens, sort_cjk_reading_order, DetectedTextBox, MockOcrBackend,
    OcrBackend, OcrResult, OcrsBackend, TesseractBackend, TextOrientation,
};
pub use dictionary::{JapaneseDictionary, MorphemeCard};
pub use translation::{
    parse_speaker_tags, BracketStyle, DeepLTranslationBackend, DialogueTurn, FuzzyLruCache,
    GoogleTranslateBackend, MockTranslationBackend, OfflineTranslationBackend,
    OllamaTranslationBackend, OpenAiTranslationBackend, ParsedDialogueLine, RateLimitManager,
    TranslationBackend, TranslationContext, DEFAULT_CACHE_CAPACITY, DEFAULT_SIMILARITY_THRESHOLD,
};
pub use shortcuts::{GlobalShortcutsManager, ShortcutAction};
pub use ipc::{default_socket_path, HudIpcClient, HudIpcServer, HudMessage, HudPosition};
pub use orchestrator::{
    spawn_orchestrator, OrchestratorConfig, OrchestratorEvent, OrchestratorHandle,
};
pub use error::{CaptureError, Result};
