use super::{apply_glossary, BoxFuture, TranslationBackend, TranslationContext};
use serde::{Deserialize, Serialize};

/// Translation backend powered by the official DeepL API (Free and Pro).
#[derive(Debug, Clone)]
pub struct DeepLTranslationBackend {
    client: reqwest::Client,
    api_key: String,
    endpoint: String,
}

#[derive(Serialize)]
struct DeepLRequest<'a> {
    text: Vec<&'a str>,
    target_lang: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_lang: Option<&'a str>,
}

#[derive(Deserialize)]
struct DeepLResponse {
    translations: Vec<DeepLTranslation>,
}

#[derive(Deserialize)]
struct DeepLTranslation {
    text: String,
}

impl DeepLTranslationBackend {
    pub const FREE_ENDPOINT: &'static str = "https://api-free.deepl.com/v2/translate";
    pub const PRO_ENDPOINT: &'static str = "https://api.deepl.com/v2/translate";

    pub fn new(api_key: impl Into<String>) -> Self {
        let key = api_key.into();
        let endpoint = if key.ends_with(":fx") {
            Self::FREE_ENDPOINT.to_string()
        } else {
            Self::PRO_ENDPOINT.to_string()
        };

        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(6))
                .build()
                .unwrap_or_default(),
            api_key: key,
            endpoint,
        }
    }

    pub fn with_custom_endpoint(api_key: impl Into<String>, endpoint: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(6))
                .build()
                .unwrap_or_default(),
            api_key: api_key.into(),
            endpoint: endpoint.into(),
        }
    }
}

impl TranslationBackend for DeepLTranslationBackend {
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

            let sl = if src_lang.is_empty() || src_lang.eq_ignore_ascii_case("auto") {
                None
            } else {
                Some(src_lang.to_uppercase())
            };

            let tl = if target_lang.is_empty() {
                "TR".to_string()
            } else {
                target_lang.to_uppercase()
            };

            let req_body = DeepLRequest {
                text: vec![trimmed],
                target_lang: &tl,
                source_lang: sl.as_deref(),
            };

            let resp = self
                .client
                .post(&self.endpoint)
                .header("Authorization", format!("DeepL-Auth-Key {}", self.api_key))
                .json(&req_body)
                .send()
                .await?;

            if !resp.status().is_success() {
                anyhow::bail!("DeepL API HTTP error: {}", resp.status());
            }

            let parsed: DeepLResponse = resp.json().await?;
            let translated = parsed
                .translations
                .into_iter()
                .next()
                .map(|t| t.text)
                .unwrap_or_default();

            let final_text = apply_glossary(&translated, ctx.glossary());
            Ok(final_text)
        })
    }
}
