use super::{apply_glossary, BoxFuture, TranslationBackend, TranslationContext};
use serde::{Deserialize, Serialize};

/// Generic translation backend supporting standard OpenAI-compatible ChatCompletions APIs.
/// Compatible with OpenAI, Groq, OpenRouter, vLLM, LM Studio, and LocalAI.
#[derive(Debug, Clone)]
pub struct OpenAiTranslationBackend {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
}

#[derive(Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

impl OpenAiTranslationBackend {
    pub const DEFAULT_ENDPOINT: &'static str = "https://api.openai.com/v1/chat/completions";
    pub const DEFAULT_MODEL: &'static str = "gpt-4o-mini";

    pub fn new(
        endpoint: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        let ep = endpoint.into();
        let formatted_endpoint = if ep.ends_with("/chat/completions") {
            ep
        } else {
            format!("{}/chat/completions", ep.trim_end_matches('/'))
        };

        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            endpoint: formatted_endpoint,
            api_key: api_key.into(),
            model: model.into(),
        }
    }
}

impl TranslationBackend for OpenAiTranslationBackend {
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

            let system_prompt = ctx.build_system_prompt(src_lang, target_lang);

            let req_body = OpenAiChatRequest {
                model: &self.model,
                messages: vec![
                    ChatMessage {
                        role: "system",
                        content: &system_prompt,
                    },
                    ChatMessage {
                        role: "user",
                        content: trimmed,
                    },
                ],
                temperature: 0.3,
            };

            let mut req = self.client.post(&self.endpoint).json(&req_body);
            if !self.api_key.trim().is_empty() {
                req = req.header("Authorization", format!("Bearer {}", self.api_key.trim()));
            }

            let resp = req.send().await?;
            if !resp.status().is_success() {
                anyhow::bail!(
                    "OpenAI API error HTTP {}: {}",
                    resp.status(),
                    resp.text().await.unwrap_or_default()
                );
            }

            let chat_resp: OpenAiChatResponse = resp.json().await?;
            let raw_translated = chat_resp
                .choices
                .into_iter()
                .next()
                .and_then(|c| c.message.content)
                .unwrap_or_default();

            let cleaned = raw_translated.trim().trim_matches('"').to_string();
            let final_text = apply_glossary(&cleaned, ctx.glossary());
            Ok(final_text)
        })
    }
}
