use super::{apply_glossary, BoxFuture, TranslationBackend, TranslationContext};
use serde_json::Value;

/// Fast, keyless translation backend using Google Translate client endpoint.
#[derive(Debug, Clone)]
pub struct GoogleTranslateBackend {
    client: reqwest::Client,
}

impl Default for GoogleTranslateBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleTranslateBackend {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

impl TranslationBackend for GoogleTranslateBackend {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        src_lang: &'a str,
        target_lang: &'a str,
        ctx: &'a TranslationContext,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        Box::pin(async move {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(String::new());
            }

            // Check if full text matches a glossary term
            if let Some(replacement) = ctx.glossary().get(trimmed) {
                return Ok(replacement.clone());
            }
            for (term, replacement) in ctx.glossary() {
                if term.eq_ignore_ascii_case(trimmed) {
                    return Ok(replacement.clone());
                }
            }

            let sl = if src_lang.is_empty() || src_lang == "auto" { "auto" } else { src_lang };
            let tl = if target_lang.is_empty() { "tr" } else { target_lang };

            let url = "https://clients5.google.com/translate_a/t";
            let resp = self
                .client
                .get(url)
                .query(&[
                    ("client", "dict-chrome-ex"),
                    ("sl", sl),
                    ("tl", tl),
                    ("q", trimmed),
                ])
                .send()
                .await?;

            if !resp.status().is_success() {
                anyhow::bail!("Google Translate error HTTP {}", resp.status());
            }

            let val: Value = resp.json().await?;
            let mut translated_text = String::new();

            // Format can be ["Translated"] or [["Translated", "src_lang"]]
            if let Some(arr) = val.as_array() {
                if let Some(first) = arr.first() {
                    if let Some(s) = first.as_str() {
                        translated_text = s.to_string();
                    } else if let Some(sub_arr) = first.as_array() {
                        if let Some(sub_first) = sub_arr.first() {
                            if let Some(s) = sub_first.as_str() {
                                translated_text = s.to_string();
                            }
                        }
                    }
                }
            }

            if translated_text.is_empty() {
                anyhow::bail!("Failed to parse Google Translate response: {val:?}");
            }

            // Apply domain glossary replacements
            let translated_text = apply_glossary(&translated_text, ctx.glossary());

            Ok(translated_text)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_google_translate_live() {
        let backend = GoogleTranslateBackend::new();
        let ctx = TranslationContext::default();
        let result = backend
            .translate("Hello world", "en", "tr", &ctx)
            .await
            .expect("Google Translate request failed");
        assert_eq!(result.to_lowercase(), "selam dünya".to_lowercase());
    }

    #[tokio::test]
    async fn test_google_translate_glossary_override() {
        let backend = GoogleTranslateBackend::new();
        let mut ctx = TranslationContext::default();
        ctx.set_glossary_term("Sandevistan", "Zaman Yavaşlatıcı");

        // Direct full text glossary match bypasses remote network call
        let result = backend
            .translate("Sandevistan", "en", "tr", &ctx)
            .await
            .expect("Translation failed");
        assert_eq!(result, "Zaman Yavaşlatıcı");
    }

    #[tokio::test]
    async fn test_google_translate_glossary_in_sentence() {
        let backend = GoogleTranslateBackend::new();
        let mut ctx = TranslationContext::default();
        ctx.set_glossary_term("selam", "merhaba");

        let result = backend
            .translate("Hello world", "en", "tr", &ctx)
            .await
            .expect("Translation failed");
        assert!(result.to_lowercase().contains("merhaba"));
    }
}
