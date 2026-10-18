use std::collections::VecDeque;
use strsim::normalized_levenshtein;

/// Default similarity threshold (90%) for accepting fuzzy matches.
pub const DEFAULT_SIMILARITY_THRESHOLD: f64 = 0.90;

/// Default cache capacity.
pub const DEFAULT_CACHE_CAPACITY: usize = 128;

#[derive(Debug, Clone)]
struct CacheEntry {
    original: String,
    translated: String,
}

/// An LRU cache that provides exact and fuzzy text lookups.
///
/// Designed to absorb OCR jitter (e.g., minor character/punctuation differences)
/// and avoid repetitive remote LLM translation calls for identical or near-identical dialogue.
#[derive(Debug, Clone)]
pub struct FuzzyLruCache {
    entries: VecDeque<CacheEntry>,
    capacity: usize,
    similarity_threshold: f64,
}

impl Default for FuzzyLruCache {
    fn default() -> Self {
        Self::new(DEFAULT_CACHE_CAPACITY, DEFAULT_SIMILARITY_THRESHOLD)
    }
}

impl FuzzyLruCache {
    /// Creates a new `FuzzyLruCache` with the given capacity and similarity threshold.
    ///
    /// `similarity_threshold` must be between `0.0` and `1.0`.
    pub fn new(capacity: usize, similarity_threshold: f64) -> Self {
        let safe_capacity = capacity.max(1);
        let safe_threshold = similarity_threshold.clamp(0.0, 1.0);

        Self {
            entries: VecDeque::with_capacity(safe_capacity),
            capacity: safe_capacity,
            similarity_threshold: safe_threshold,
        }
    }

    /// Creates a new `FuzzyLruCache` with default 90% similarity threshold.
    pub fn with_default_threshold(capacity: usize) -> Self {
        Self::new(capacity, DEFAULT_SIMILARITY_THRESHOLD)
    }

    /// Returns the number of cached translations.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Checks if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clears all entries from the cache.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Retrieves a translation from the cache using exact or fuzzy matching.
    ///
    /// On match, the matched entry is moved to the most recently used (front) position.
    pub fn get(&mut self, text: &str) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        // 1. Fast path: exact match
        if let Some(pos) = self.entries.iter().position(|e| e.original == trimmed) {
            let entry = self.entries.remove(pos)?;
            let translation = entry.translated.clone();
            self.entries.push_front(entry);
            return Some(translation);
        }

        // 2. Fuzzy path: scan for best matching candidate above threshold
        let mut best_idx = None;
        let mut best_score = 0.0;

        for (idx, entry) in self.entries.iter().enumerate() {
            let score = normalized_levenshtein(trimmed, &entry.original);
            if score >= self.similarity_threshold && score > best_score {
                best_score = score;
                best_idx = Some(idx);
            }
        }

        if let Some(idx) = best_idx {
            let entry = self.entries.remove(idx)?;
            let translation = entry.translated.clone();
            self.entries.push_front(entry);
            Some(translation)
        } else {
            None
        }
    }

    /// Inspects the cache for a match without modifying the LRU order.
    ///
    /// Returns `Some((translation, similarity_score))` if a match meets the threshold.
    pub fn peek(&self, text: &str) -> Option<(&str, f64)> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Exact match
        if let Some(entry) = self.entries.iter().find(|e| e.original == trimmed) {
            return Some((&entry.translated, 1.0));
        }

        // Fuzzy match
        let mut best_entry = None;
        let mut best_score = 0.0;

        for entry in &self.entries {
            let score = normalized_levenshtein(trimmed, &entry.original);
            if score >= self.similarity_threshold && score > best_score {
                best_score = score;
                best_entry = Some(entry);
            }
        }

        best_entry.map(|entry| (entry.translated.as_str(), best_score))
    }

    /// Inserts or updates a translation in the cache.
    ///
    /// If an exact match exists, its translation is updated and moved to the front.
    /// If capacity is exceeded, the least recently used entry is evicted.
    pub fn insert(&mut self, original: impl Into<String>, translated: impl Into<String>) {
        let original = original.into().trim().to_string();
        let translated = translated.into().trim().to_string();

        if original.is_empty() {
            return;
        }

        // Update existing exact entry if present
        if let Some(pos) = self.entries.iter().position(|e| e.original == original) {
            self.entries.remove(pos);
        } else if self.entries.len() >= self.capacity {
            self.entries.pop_back();
        }

        self.entries.push_front(CacheEntry {
            original,
            translated,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let mut cache = FuzzyLruCache::new(5, 0.90);
        cache.insert("Hello brave knight!", "Merhaba cesur şövalye!");

        assert_eq!(
            cache.get("Hello brave knight!"),
            Some("Merhaba cesur şövalye!".to_string())
        );
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_fuzzy_match_jitter_tolerance() {
        let mut cache = FuzzyLruCache::new(5, 0.90);
        // Original stored dialogue
        cache.insert(
            "Press the button to initiate warp jump sequence.",
            "Warp atlama dizilimini başlatmak için düğmeye basın.",
        );

        // OCR jitter: missing period
        let jittered = "Press the button to initiate warp jump sequence";
        let score = normalized_levenshtein(
            "Press the button to initiate warp jump sequence.",
            jittered,
        );
        assert!(score >= 0.95);

        let hit = cache.get(jittered);
        assert_eq!(
            hit,
            Some("Warp atlama dizilimini başlatmak için düğmeye basın.".to_string())
        );
    }

    #[test]
    fn test_fuzzy_miss_on_different_text() {
        let mut cache = FuzzyLruCache::new(5, 0.90);
        cache.insert("You obtained 50 Gold Coins!", "50 Altın Sikke kazandınız!");

        assert_eq!(cache.get("You defeated the Goblin Chief!"), None);
    }

    #[test]
    fn test_lru_eviction() {
        let mut cache = FuzzyLruCache::new(2, 0.90);
        cache.insert("First sentence.", "İlk cümle.");
        cache.insert("Second sentence.", "İkinci cümle.");
        assert_eq!(cache.len(), 2);

        // Access "First sentence." so it becomes MRU
        assert!(cache.get("First sentence.").is_some());

        // Insert third sentence -> "Second sentence." should be evicted
        cache.insert("Third sentence.", "Üçüncü cümle.");
        assert_eq!(cache.len(), 2);

        assert!(cache.get("First sentence.").is_some());
        assert!(cache.get("Third sentence.").is_some());
        assert_eq!(cache.get("Second sentence."), None);
    }

    #[test]
    fn test_peek_without_modifying_lru() {
        let cache = FuzzyLruCache::new(2, 0.90);
        assert_eq!(cache.peek("Empty"), None);
    }
}
