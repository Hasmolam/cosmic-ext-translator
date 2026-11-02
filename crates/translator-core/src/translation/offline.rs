use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::translation::{apply_glossary, BoxFuture, TranslationBackend, TranslationContext};

/// Returns the default directory for offline translation models following XDG Base Directory specification.
pub fn default_offline_model_dir() -> PathBuf {
    if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
        if !data_home.trim().is_empty() {
            return PathBuf::from(data_home)
                .join("cosmic-ext-lingua")
                .join("models")
                .join("translation");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join("cosmic-ext-lingua")
                .join("models")
                .join("translation");
        }
    }
    PathBuf::from("/tmp/cosmic-ext-lingua/models/translation")
}

/// Offline translation engine operating without external network access.
///
/// Supports local neural model paths under `$XDG_DATA_HOME/cosmic-ext-lingua/models/translation/`
/// and provides local rule/dictionary translation fallback.
#[derive(Debug, Clone)]
pub struct OfflineTranslationBackend {
    model_name: String,
    model_dir: PathBuf,
    rules: HashMap<String, String>,
}

impl Default for OfflineTranslationBackend {
    fn default() -> Self {
        Self::new("marian-local")
    }
}

impl OfflineTranslationBackend {
    /// Creates a new `OfflineTranslationBackend` for the given model identifier.
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            model_dir: default_offline_model_dir(),
            rules: HashMap::new(),
        }
    }

    /// Sets a custom model directory path.
    pub fn with_model_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.model_dir = path.into();
        self
    }

    /// Returns the configured model name.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Returns the model directory path.
    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// Returns the resolved path to the model file or directory.
    pub fn model_path(&self) -> PathBuf {
        self.model_dir.join(&self.model_name)
    }

    /// Checks if the local model file or folder is present on disk.
    pub fn is_model_available(&self) -> bool {
        let path = self.model_path();
        path.exists()
            || self.model_dir.join(format!("{}.bin", self.model_name)).exists()
            || self.model_dir.join(format!("{}.onnx", self.model_name)).exists()
    }

    /// Registers a custom phrase or sentence translation rule.
    pub fn add_rule(&mut self, source: impl Into<String>, target: impl Into<String>) {
        self.rules.insert(source.into(), target.into());
    }

    /// Builder pattern helper to register a rule.
    pub fn with_rule(mut self, source: impl Into<String>, target: impl Into<String>) -> Self {
        self.add_rule(source, target);
        self
    }

    /// Builder pattern helper to set multiple translation rules.
    pub fn with_rules(mut self, rules: HashMap<String, String>) -> Self {
        self.rules.extend(rules);
        self
    }
}

impl TranslationBackend for OfflineTranslationBackend {
    fn translate<'a>(
        &'a self,
        text: &'a str,
        _src_lang: &'a str,
        target_lang: &'a str,
        ctx: &'a TranslationContext,
    ) -> BoxFuture<'a, anyhow::Result<String>> {
        let trimmed = text.trim();

        // 1. Check exact rule match
        if let Some(target) = self.rules.get(trimmed) {
            let res = target.clone();
            return Box::pin(async move { Ok(res) });
        }

        // 2. Apply context glossary if present
        let mut processed = trimmed.to_string();
        if !ctx.glossary().is_empty() {
            processed = apply_glossary(&processed, ctx.glossary());
        }

        // 3. Apply custom rules as substring replacements
        for (src, tr) in &self.rules {
            if processed.contains(src) {
                processed = processed.replace(src, tr);
            }
        }

        // 4. Return offline translation
        let output = format!("[Offline: {target_lang}] {processed}");
        Box::pin(async move { Ok(output) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_offline_backend_rules_and_glossary() {
        let mut backend = OfflineTranslationBackend::new("test-model");
        backend.add_rule("Hello", "Merhaba");

        let mut ctx = TranslationContext::default();
        ctx.set_glossary_term("Hero", "Kahraman");

        // Exact rule match
        let res1 = backend.translate("Hello", "en", "tr", &ctx).await.unwrap();
        assert_eq!(res1, "Merhaba");

        // Substring rule and glossary replacement
        let res2 = backend.translate("Hero says Hello!", "en", "tr", &ctx).await.unwrap();
        assert!(res2.contains("Kahraman"));
        assert!(res2.contains("Merhaba"));
    }

    #[test]
    fn test_offline_model_path_resolution() {
        let backend = OfflineTranslationBackend::new("marian-ja-en")
            .with_model_dir("/opt/models/translation");
        assert_eq!(backend.model_path(), PathBuf::from("/opt/models/translation/marian-ja-en"));
        assert!(!backend.is_model_available());
    }
}
