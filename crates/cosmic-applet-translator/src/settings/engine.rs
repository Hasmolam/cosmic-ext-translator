//! Engine page: translation engine, languages, OCR engine and engine credentials.

use cosmic::iced::{Alignment, Task};
use cosmic::widget::{self, button, row, text};
use cosmic::Element;
use translator_core::ocr::{paddle, OcrEngineChoice, OrientationMode};
use translator_core::translation::OllamaTranslationBackend;
use translator_core::{SourceLanguage, TargetLanguage};

use super::{choice_button, Update};
use crate::config::Config;
use crate::fl;

/// Translation engines, as `(config code, label)`.
const ENGINES: [(&str, &str); 4] = [
    ("google", "Google"),
    ("deepl", "DeepL"),
    ("ollama", "Ollama"),
    ("openai", "OpenAI/LLM"),
];

#[derive(Debug, Clone, Default)]
pub struct State {
    pub ollama_test_status: Option<String>,
    pub is_testing_ollama: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    EngineSelected(String),
    TargetLangChanged(String),
    SourceLangChanged(String),
    OcrOrientationChanged(String),
    OcrEngineChanged(String),
    DeepLApiKeyChanged(String),
    OpenAiEndpointChanged(String),
    OpenAiApiKeyChanged(String),
    OpenAiModelChanged(String),
    OllamaEndpointChanged(String),
    OllamaModelChanged(String),
    TestOllamaConnection,
    OllamaTestResult(Result<Vec<String>, String>),
}

pub fn update(state: &mut State, config: &mut Config, message: Message) -> Update<Message> {
    match message {
        Message::EngineSelected(engine) => config.engine = engine,
        Message::TargetLangChanged(lang) => config.target_lang = lang,
        Message::SourceLangChanged(lang) => config.source_lang = lang,
        Message::OcrOrientationChanged(mode) => config.ocr_orientation = mode,
        Message::OcrEngineChanged(engine) => config.ocr_engine = engine,
        Message::DeepLApiKeyChanged(key) => config.deepl_api_key = key,
        Message::OpenAiEndpointChanged(endpoint) => config.openai_endpoint = endpoint,
        Message::OpenAiApiKeyChanged(key) => config.openai_api_key = key,
        Message::OpenAiModelChanged(model) => config.openai_model = model,
        Message::OllamaEndpointChanged(endpoint) => config.ollama_endpoint = endpoint,
        Message::OllamaModelChanged(model) => config.ollama_model = model,
        Message::TestOllamaConnection => {
            state.is_testing_ollama = true;
            let endpoint = config.ollama_endpoint.clone();
            let model = config.ollama_model.clone();
            return Update::task(Task::perform(
                async move {
                    OllamaTranslationBackend::new(endpoint, model)
                        .test_connection()
                        .await
                        .map_err(|err| err.to_string())
                },
                Message::OllamaTestResult,
            ));
        }
        Message::OllamaTestResult(result) => {
            state.is_testing_ollama = false;
            state.ollama_test_status = Some(match result {
                Ok(models) => format!("✓ {}", fl!("connection-success", count = models.len())),
                Err(err) => format!("✗ {}", fl!("connection-error", error = err.as_str())),
            });
        }
    }
    Update::none()
}

pub fn view<'a>(state: &'a State, config: &'a Config) -> Element<'a, Message> {
    let active_engine = config.engine.as_str();
    let mut engine_buttons = row![].spacing(6).align_y(Alignment::Center);
    for (code, label) in ENGINES {
        engine_buttons = engine_buttons.push(choice_button(
            label.to_string(),
            active_engine == code,
            Message::EngineSelected(code.to_string()),
        ));
    }

    let mut section = widget::settings::section()
        .title(fl!("engine-selection"))
        .add(widget::settings::item(fl!("active-engine"), engine_buttons))
        .add(widget::settings::item(
            fl!("target-language"),
            target_language_dropdown(&config.target_lang),
        ))
        .add(widget::settings::item(
            fl!("source-language"),
            widget::dropdown(
                source_language_labels(),
                SourceLanguage::from_code(&config.source_lang).map(SourceLanguage::index),
                |idx| {
                    let lang = SourceLanguage::ALL
                        .get(idx)
                        .copied()
                        .unwrap_or(SourceLanguage::Auto);
                    Message::SourceLangChanged(lang.code().to_string())
                },
            ),
        ));

    let source_script = SourceLanguage::from_code(&config.source_lang).map(|lang| lang.script());
    if source_script.is_some_and(paddle::supports_script) {
        section = section.add(widget::settings::item(
            fl!("ocr-engine"),
            widget::dropdown(
                ocr_engine_labels(),
                Some(OcrEngineChoice::from_code(&config.ocr_engine).index()),
                |idx| {
                    let choice = OcrEngineChoice::ALL
                        .get(idx)
                        .copied()
                        .unwrap_or(OcrEngineChoice::Auto);
                    Message::OcrEngineChanged(choice.code().to_string())
                },
            ),
        ));
    }

    if source_script.is_some_and(|script| script.is_cjk()) {
        section = section.add(widget::settings::item(
            fl!("text-direction"),
            widget::dropdown(
                orientation_labels(),
                Some(OrientationMode::from_code(&config.ocr_orientation).index()),
                |idx| {
                    let mode = OrientationMode::ALL
                        .get(idx)
                        .copied()
                        .unwrap_or(OrientationMode::Auto);
                    Message::OcrOrientationChanged(mode.code().to_string())
                },
            ),
        ));
    }

    let section = match active_engine {
        "google" => section.add(widget::settings::item(
            fl!("engine-info"),
            text(fl!("engine-info-google")).size(13),
        )),
        "deepl" => section.add(widget::settings::item(
            fl!("deepl-api-key"),
            widget::text_input(fl!("deepl-api-key-placeholder"), &config.deepl_api_key)
                .on_input(Message::DeepLApiKeyChanged)
                .width(280),
        )),
        "openai" => section
            .add(widget::settings::item(
                fl!("api-endpoint"),
                widget::text_input("https://api.openai.com/v1", &config.openai_endpoint)
                    .on_input(Message::OpenAiEndpointChanged)
                    .width(280),
            ))
            .add(widget::settings::item(
                fl!("model-name"),
                widget::text_input("gpt-4o-mini", &config.openai_model)
                    .on_input(Message::OpenAiModelChanged)
                    .width(280),
            ))
            .add(widget::settings::item(
                fl!("api-key"),
                widget::text_input("sk-...", &config.openai_api_key)
                    .on_input(Message::OpenAiApiKeyChanged)
                    .width(280),
            )),
        _ => {
            let status = if state.is_testing_ollama {
                fl!("testing-connection")
            } else if let Some(status) = &state.ollama_test_status {
                status.clone()
            } else {
                fl!("status-unknown")
            };
            section
                .add(widget::settings::item(
                    fl!("ollama-endpoint"),
                    widget::text_input("http://localhost:11434", &config.ollama_endpoint)
                        .on_input(Message::OllamaEndpointChanged)
                        .width(260),
                ))
                .add(widget::settings::item(
                    fl!("ollama-model"),
                    widget::text_input("qwen2.5:3b", &config.ollama_model)
                        .on_input(Message::OllamaModelChanged)
                        .width(260),
                ))
                .add(widget::settings::item(
                    fl!("connection-test"),
                    row![
                        text(status).size(13),
                        button::suggested(fl!("test-button"))
                            .on_press(Message::TestOllamaConnection),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                ))
        }
    };

    section.into()
}

/// Target language selection. A stored code outside [`TargetLanguage::ALL`] is kept and
/// shown as an extra entry, so it is not replaced until another language is picked.
fn target_language_dropdown(current: &str) -> Element<'static, Message> {
    let (labels, selected) = target_language_options(current);
    let current = current.to_string();
    widget::dropdown(labels, selected, move |idx| {
        let code = TargetLanguage::ALL
            .get(idx)
            .map_or_else(|| current.clone(), |lang| lang.code().to_string());
        Message::TargetLangChanged(code)
    })
    .into()
}

/// Dropdown labels in [`TargetLanguage::ALL`] order, plus an entry for an unknown
/// stored code, and the index of the current selection.
fn target_language_options(current: &str) -> (Vec<String>, Option<usize>) {
    let mut labels: Vec<String> = TargetLanguage::ALL
        .iter()
        .map(|lang| match lang {
            TargetLanguage::Turkish => fl!("target-lang-tr"),
            TargetLanguage::English => fl!("target-lang-en"),
            TargetLanguage::German => fl!("target-lang-de"),
            TargetLanguage::French => fl!("target-lang-fr"),
            TargetLanguage::Spanish => fl!("target-lang-es"),
            TargetLanguage::Italian => fl!("target-lang-it"),
            TargetLanguage::Portuguese => fl!("target-lang-pt"),
            TargetLanguage::Russian => fl!("target-lang-ru"),
            TargetLanguage::Ukrainian => fl!("target-lang-uk"),
            TargetLanguage::Polish => fl!("target-lang-pl"),
            TargetLanguage::Dutch => fl!("target-lang-nl"),
            TargetLanguage::Arabic => fl!("target-lang-ar"),
            TargetLanguage::Japanese => fl!("target-lang-ja"),
            TargetLanguage::ChineseSimplified => fl!("target-lang-zh"),
            TargetLanguage::Korean => fl!("target-lang-ko"),
        })
        .collect();

    let selected = match TargetLanguage::from_code(current) {
        Some(lang) => Some(lang.index()),
        None if current.trim().is_empty() => None,
        None => {
            labels.push(fl!("target-lang-custom", code = current.trim()));
            Some(labels.len() - 1)
        }
    };
    (labels, selected)
}

/// Localised dropdown labels, in [`OcrEngineChoice::ALL`] order.
fn ocr_engine_labels() -> Vec<String> {
    OcrEngineChoice::ALL
        .iter()
        .map(|choice| match choice {
            OcrEngineChoice::Auto => fl!("ocr-engine-auto"),
            OcrEngineChoice::Paddle => fl!("ocr-engine-paddle"),
            OcrEngineChoice::Tesseract => fl!("ocr-engine-tesseract"),
        })
        .collect()
}

/// Localised dropdown labels, in [`OrientationMode::ALL`] order.
fn orientation_labels() -> Vec<String> {
    OrientationMode::ALL
        .iter()
        .map(|mode| match mode {
            OrientationMode::Auto => fl!("text-direction-auto"),
            OrientationMode::Horizontal => fl!("text-direction-horizontal"),
            OrientationMode::Vertical => fl!("text-direction-vertical"),
        })
        .collect()
}

/// Localised dropdown labels, in [`SourceLanguage::ALL`] order.
fn source_language_labels() -> Vec<String> {
    SourceLanguage::ALL
        .iter()
        .map(|lang| match lang {
            SourceLanguage::Auto => fl!("lang-auto"),
            SourceLanguage::English => fl!("lang-en"),
            SourceLanguage::Japanese => fl!("lang-ja"),
            SourceLanguage::ChineseSimplified => fl!("lang-zh"),
            SourceLanguage::Korean => fl!("lang-ko"),
            SourceLanguage::German => fl!("lang-de"),
            SourceLanguage::French => fl!("lang-fr"),
            SourceLanguage::Spanish => fl!("lang-es"),
            SourceLanguage::Italian => fl!("lang-it"),
            SourceLanguage::Portuguese => fl!("lang-pt"),
            SourceLanguage::Turkish => fl!("lang-tr"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_result_clears_testing_flag() {
        crate::i18n::init(&[]);
        let mut state = State {
            is_testing_ollama: true,
            ..State::default()
        };
        let mut config = Config::default();
        let _ = update(
            &mut state,
            &mut config,
            Message::OllamaTestResult(Err("refused".into())),
        );
        assert!(!state.is_testing_ollama);
        assert!(state
            .ollama_test_status
            .as_deref()
            .is_some_and(|s| s.contains("refused")));
    }

    #[test]
    fn known_target_is_selected() {
        crate::i18n::init(&[]);
        let (labels, selected) = target_language_options("de");
        assert_eq!(labels.len(), TargetLanguage::ALL.len());
        assert_eq!(selected, Some(TargetLanguage::German.index()));
    }

    #[test]
    fn unknown_target_is_kept_as_extra_entry() {
        crate::i18n::init(&[]);
        let (labels, selected) = target_language_options("sv");
        assert_eq!(labels.len(), TargetLanguage::ALL.len() + 1);
        assert_eq!(selected, Some(TargetLanguage::ALL.len()));
        assert!(labels.last().is_some_and(|l| l.contains("sv")));

        let (labels, selected) = target_language_options("");
        assert_eq!(labels.len(), TargetLanguage::ALL.len());
        assert_eq!(selected, None);
    }
}
