use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const DEFAULT_MAX_HISTORY_ENTRIES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub id: u64,
    pub timestamp_str: String,
    pub speaker: Option<String>,
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogueBacklog {
    pub entries: VecDeque<HistoryEntry>,
    pub max_entries: usize,
    next_id: u64,
}

impl Default for DialogueBacklog {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_HISTORY_ENTRIES)
    }
}

impl DialogueBacklog {
    #[must_use]
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
            next_id: 1,
        }
    }

    pub fn push(&mut self, speaker: Option<String>, source: String, target: String) {
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = duration.as_secs();
        let hours = (secs / 3600) % 24;
        let minutes = (secs / 60) % 60;
        let seconds = secs % 60;
        let timestamp_str = format!("{hours:02}:{minutes:02}:{seconds:02}");

        self.push_with_timestamp(timestamp_str, speaker, source, target);
    }

    pub fn push_with_timestamp(
        &mut self,
        timestamp_str: String,
        speaker: Option<String>,
        source: String,
        target: String,
    ) {
        if self.max_entries > 0 && self.entries.len() >= self.max_entries {
            self.entries.pop_back();
        }

        self.entries.push_front(HistoryEntry {
            id: self.next_id,
            timestamp_str,
            speaker,
            source,
            target,
        });
        self.next_id = self.next_id.wrapping_add(1);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn entries(&self) -> &VecDeque<HistoryEntry> {
        &self.entries
    }

    #[must_use]
    pub fn search<'a>(&'a self, query: &str) -> Vec<&'a HistoryEntry> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.entries.iter().collect();
        }

        self.entries
            .iter()
            .filter(|entry| {
                entry.source.to_lowercase().contains(&q)
                    || entry.target.to_lowercase().contains(&q)
                    || entry
                        .speaker
                        .as_ref()
                        .map(|s| s.to_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fifo_eviction_at_capacity() {
        // Small capacity test
        let mut backlog = DialogueBacklog::new(3);
        backlog.push_with_timestamp("12:00:01".into(), None, "Line 1".into(), "Satır 1".into());
        backlog.push_with_timestamp("12:00:02".into(), None, "Line 2".into(), "Satır 2".into());
        backlog.push_with_timestamp("12:00:03".into(), None, "Line 3".into(), "Satır 3".into());

        assert_eq!(backlog.entries().len(), 3);
        assert_eq!(backlog.entries()[0].source, "Line 3");
        assert_eq!(backlog.entries()[2].source, "Line 1");

        // Pushing 4th item evicts Line 1
        backlog.push_with_timestamp("12:00:04".into(), None, "Line 4".into(), "Satır 4".into());
        assert_eq!(backlog.entries().len(), 3);
        assert_eq!(backlog.entries()[0].source, "Line 4");
        assert_eq!(backlog.entries()[1].source, "Line 3");
        assert_eq!(backlog.entries()[2].source, "Line 2");

        // Default 500 capacity test
        let mut large_backlog = DialogueBacklog::default();
        for i in 1..=600 {
            large_backlog.push_with_timestamp(
                "12:00:00".into(),
                None,
                format!("Source {i}"),
                format!("Hedef {i}"),
            );
        }
        assert_eq!(large_backlog.entries().len(), 500);
        assert_eq!(large_backlog.entries()[0].source, "Source 600");
        assert_eq!(large_backlog.entries()[499].source, "Source 101");
    }

    #[test]
    fn test_speaker_attribution() {
        let mut backlog = DialogueBacklog::new(10);
        backlog.push_with_timestamp(
            "10:15:30".into(),
            Some("Hero".into()),
            "I will save the world!".into(),
            "Dünyayı kurtaracağım!".into(),
        );
        backlog.push_with_timestamp(
            "10:15:45".into(),
            None,
            "A gust of wind blew.".into(),
            "Bir rüzgar esti.".into(),
        );

        let entries = backlog.entries();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].speaker, None);
        assert_eq!(entries[1].speaker, Some("Hero".to_string()));
        assert_eq!(entries[1].id, 1);
        assert_eq!(entries[0].id, 2);
    }

    #[test]
    fn test_search_filtering() {
        let mut backlog = DialogueBacklog::new(10);
        backlog.push_with_timestamp(
            "11:00:00".into(),
            Some("Knight".into()),
            "Draw your sword!".into(),
            "Kılıcını çek!".into(),
        );
        backlog.push_with_timestamp(
            "11:01:00".into(),
            Some("Mage".into()),
            "Cast fire spell!".into(),
            "Ateş büyüsü yap!".into(),
        );
        backlog.push_with_timestamp(
            "11:02:00".into(),
            None,
            "The dragon roared loudly.".into(),
            "Ejderha gürültüyle kükredi.".into(),
        );

        // Empty search returns all 3
        assert_eq!(backlog.search("").len(), 3);
        assert_eq!(backlog.search("   ").len(), 3);

        // Filter by speaker
        let speaker_res = backlog.search("knight");
        assert_eq!(speaker_res.len(), 1);
        assert_eq!(speaker_res[0].speaker, Some("Knight".to_string()));

        // Filter by source text
        let source_res = backlog.search("sword");
        assert_eq!(source_res.len(), 1);
        assert_eq!(source_res[0].source, "Draw your sword!");

        // Filter by target text
        let target_res = backlog.search("büyüsü");
        assert_eq!(target_res.len(), 1);
        assert_eq!(target_res[0].target, "Ateş büyüsü yap!");

        // Filter with no match
        assert!(backlog.search("spaceship").is_empty());
    }

    #[test]
    fn test_clear() {
        let mut backlog = DialogueBacklog::new(10);
        backlog.push(Some("Villager".into()), "Hello!".into(), "Merhaba!".into());
        backlog.push(None, "Silence...".into(), "Sessizlik...".into());
        assert_eq!(backlog.entries().len(), 2);

        backlog.clear();
        assert!(backlog.entries().is_empty());
        assert_eq!(backlog.entries().len(), 0);
    }
}
