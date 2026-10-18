pub mod deepl;
pub mod fuzzy_cache;
pub mod google;
pub mod openai;

pub use deepl::DeepLTranslationBackend;
pub use fuzzy_cache::{FuzzyLruCache, DEFAULT_CACHE_CAPACITY, DEFAULT_SIMILARITY_THRESHOLD};
pub use google::GoogleTranslateBackend;
pub use openai::OpenAiTranslationBackend;

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Context for translation including recent dialogue history and game glossary.
#[derive(Debug, Clone)]
pub struct TranslationContext {
    history: VecDeque<(String, String)>,
    max_history: usize,
    glossary: HashMap<String, String>,
}

impl Default for TranslationContext {
    fn default() -> Self {
        Self::new(3)
    }
}

impl TranslationContext {
    /// Creates a new `TranslationContext` retaining up to `max_history` dialogue turns.
    pub fn new(max_history: usize) -> Self {
        Self {
            history: VecDeque::with_capacity(max_history),
            max_history,
            glossary: HashMap::new(),
        }
    }

    /// Records a completed dialogue turn into history.
    pub fn add_turn(&mut self, original: impl Into<String>, translated: impl Into<String>) {
        if self.history.len() >= self.max_history && self.max_history > 0 {
            self.history.pop_back();
        }
        if self.max_history > 0 {
            self.history.push_front((original.into(), translated.into()));
        }
    }

    /// Adds or updates a term in the domain-specific glossary.
    pub fn set_glossary_term(&mut self, term: impl Into<String>, translation: impl Into<String>) {
        self.glossary.insert(term.into(), translation.into());
    }

    /// Returns the recent dialogue history (most recent first).
    pub fn history(&self) -> &VecDeque<(String, String)> {
        &self.history
    }

    /// Returns the glossary terms.
    pub fn glossary(&self) -> &HashMap<String, String> {
        &self.glossary
    }

    /// Clears dialogue history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Builds an LLM system prompt containing game localization instructions,
    /// glossary rules, and recent dialogue history for grammatical/gender continuity.
    pub fn build_system_prompt(&self, src_lang: &str, target_lang: &str) -> String {
        let mut prompt = format!(
            "You are a professional video game localization engine. \
             Translate the provided dialogue text from {src_lang} to {target_lang}.\n\
             Rules:\n\
             1. Output ONLY the translated text. Do not add quotes, markdown formatting, explanations, or notes.\n\
             2. Maintain the emotional tone, speaker brevity, and gaming context.\n"
        );

        if !self.glossary.is_empty() {
            prompt.push_str("\n[Glossary / Preserved Terms]:\n");
            for (term, tr) in &self.glossary {
                prompt.push_str(&format!("- \"{term}\" -> \"{tr}\"\n"));
            }
        }

        if !self.history.is_empty() {
            prompt.push_str("\n[Previous Dialogue Context (oldest to newest)]:\n");
            // Reverse so model reads context chronologically
            for (orig, tr) in self.history.iter().rev() {
                prompt.push_str(&format!("Context: \"{orig}\" => \"{tr}\"\n"));
            }
        }

        prompt
    }
}

/// Generic trait for translation backends.
pub trait TranslationBackend: Send + Sync {
    /// Translates a piece of text within a given dialogue context.
    fn translate<'a>(
        &'a self,
        text: &'a str,
        src_lang: &'a str,
        target_lang: &'a str,
        ctx: &'a TranslationContext,
    ) -> BoxFuture<'a, anyhow::Result<String>>;
}

/// A mock backend returning formatted static text (for tests & offline runs).
#[derive(Debug, Clone)]
pub struct MockTranslationBackend {
    pub prefix: String,
}

impl Default for MockTranslationBackend {
    fn default() -> Self {
        Self {
            prefix: "TR".to_string(),
        }
    }
}

impl MockTranslationBackend {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }
}

impl TranslationBackend for MockTranslationBackend {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        _src_lang: &'a str,
        target_lang: &'a str,
        _ctx: &'a TranslationContext,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        let res = format!("[{}: {}] {}", self.prefix, target_lang, text.trim());
        Box::pin(async move { Ok(res) })
    }
}

#[derive(Serialize)]
struct OllamaGenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    system: &'a str,
    stream: bool,
    options: OllamaOptions,
}

#[derive(Serialize)]
struct OllamaOptions {
    temperature: f32,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

/// Translation backend powered by a local Ollama instance (e.g., Qwen 2.5, Llama 3.2).
#[derive(Debug, Clone)]
pub struct OllamaTranslationBackend {
    client: reqwest::Client,
    endpoint: String,
    model: String,
}

impl OllamaTranslationBackend {
    pub const DEFAULT_ENDPOINT: &'static str = "http://localhost:11434";
    pub const DEFAULT_MODEL: &'static str = "qwen2.5:3b";

    pub fn new(endpoint: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            endpoint: endpoint.into().trim_end_matches('/').to_string(),
            model: model.into(),
        }
    }

    pub fn default_local() -> Self {
        Self::new(Self::DEFAULT_ENDPOINT, Self::DEFAULT_MODEL)
    }

    /// Queries the Ollama instance (/api/tags) to verify connectivity and list installed models.
    pub async fn test_connection(&self) -> anyhow::Result<Vec<String>> {
        #[derive(Deserialize)]
        struct TagsResponse {
            #[serde(default)]
            models: Vec<ModelTag>,
        }
        #[derive(Deserialize)]
        struct ModelTag {
            name: String,
        }

        let url = format!("{}/api/tags", self.endpoint);
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            anyhow::bail!("Ollama returned HTTP error status: {}", resp.status());
        }
        let tags: TagsResponse = resp.json().await?;
        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }
}

impl TranslationBackend for OllamaTranslationBackend {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        src_lang: &'a str,
        target_lang: &'a str,
        ctx: &'a TranslationContext,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        Box::pin(async move {
            let system_prompt = ctx.build_system_prompt(src_lang, target_lang);
            let url = format!("{}/api/generate", self.endpoint);

            let body = OllamaGenerateRequest {
                model: &self.model,
                prompt: text,
                system: &system_prompt,
                stream: false,
                options: OllamaOptions { temperature: 0.2 },
            };

            let response = self
                .client
                .post(&url)
                .json(&body)
                .send()
                .await?
                .error_for_status()?;

            let payload: OllamaGenerateResponse = response.json().await?;
            let mut translated = payload.response.trim().to_string();

            // Strip redundant enclosing quotation marks if generated by the LLM
            if translated.len() >= 2 && translated.starts_with('"') && translated.ends_with('"') {
                translated = translated[1..translated.len() - 1].trim().to_string();
            } else if translated.starts_with('“') && translated.ends_with('”') {
                translated = translated
                    .strip_prefix('“')
                    .and_then(|s| s.strip_suffix('”'))
                    .unwrap_or(&translated)
                    .trim()
                    .to_string();
            }

            Ok(translated)
        })
    }
}

/// Applies domain glossary replacements to a text string.
/// Longer glossary terms take precedence over shorter ones to prevent partial substring collisions.
pub fn apply_glossary(text: &str, glossary: &HashMap<String, String>) -> String {
    if glossary.is_empty() || text.is_empty() {
        return text.to_string();
    }

    let mut entries: Vec<(&String, &String)> = glossary.iter().collect();
    entries.sort_by_key(|(a, _)| std::cmp::Reverse(a.len()));

    let mut result = text.to_string();
    for (term, replacement) in entries {
        if term.is_empty() {
            continue;
        }
        if result.contains(term.as_str()) {
            result = result.replace(term.as_str(), replacement);
        } else {
            // Case-insensitive ASCII fallback match
            let term_lower = term.to_ascii_lowercase();
            let mut replaced = false;
            let mut new_result = String::with_capacity(result.len());
            let mut start = 0;
            let text_lower = result.to_ascii_lowercase();

            while let Some(idx) = text_lower[start..].find(&term_lower) {
                let actual_start = start + idx;
                let actual_end = actual_start + term.len();
                new_result.push_str(&result[start..actual_start]);
                new_result.push_str(replacement);
                start = actual_end;
                replaced = true;
            }

            if replaced {
                new_result.push_str(&result[start..]);
                result = new_result;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translation_context_history_rotation() {
        let mut ctx = TranslationContext::new(2);
        ctx.add_turn("Turn 1", "Tur 1");
        ctx.add_turn("Turn 2", "Tur 2");
        assert_eq!(ctx.history().len(), 2);

        ctx.add_turn("Turn 3", "Tur 3");
        assert_eq!(ctx.history().len(), 2);
        assert_eq!(ctx.history()[0].0, "Turn 3");
        assert_eq!(ctx.history()[1].0, "Turn 2");
    }

    #[test]
    fn test_system_prompt_builder() {
        let mut ctx = TranslationContext::new(3);
        ctx.set_glossary_term("Chocobo", "Kocakuş");
        ctx.add_turn("Look over there!", "Şuraya bak!");

        let prompt = ctx.build_system_prompt("en", "tr");
        assert!(prompt.contains("Chocobo"));
        assert!(prompt.contains("Kocakuş"));
        assert!(prompt.contains("Look over there!"));
        assert!(prompt.contains("Şuraya bak!"));
    }

    #[tokio::test]
    async fn test_mock_backend() {
        let backend = MockTranslationBackend::new("TEST");
        let ctx = TranslationContext::default();
        let result = backend
            .translate("Sword of light", "en", "tr", &ctx)
            .await
            .unwrap();

        assert_eq!(result, "[TEST: tr] Sword of light");
    }

    #[test]
    fn test_apply_glossary_precedence() {
        let mut glossary = HashMap::new();
        glossary.insert("Potion".to_string(), "İksir".to_string());
        glossary.insert("Health Potion".to_string(), "Can İksiri".to_string());

        let input = "Player used a Health Potion and another Potion.";
        let result = apply_glossary(input, &glossary);
        assert_eq!(result, "Player used a Can İksiri and another İksir.");
    }

    #[test]
    fn test_apply_glossary_case_insensitive() {
        let mut glossary = HashMap::new();
        glossary.insert("cyberware".to_string(), "Siberdonanım".to_string());

        let input = "Upgraded Cyberware installed.";
        let result = apply_glossary(input, &glossary);
        assert_eq!(result, "Upgraded Siberdonanım installed.");
    }
}
