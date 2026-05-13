//! In-memory store of captured HTTP flows.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::HttpFlow;

const DEFAULT_CAPACITY: usize = 5000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub flow: HttpFlow,
    pub note: Option<String>,
    pub starred: bool,
}

impl From<HttpFlow> for HistoryEntry {
    fn from(flow: HttpFlow) -> Self {
        Self {
            flow,
            note: None,
            starred: false,
        }
    }
}

#[derive(Clone)]
pub struct HistoryStore {
    inner: Arc<RwLock<HistoryInner>>,
}

struct HistoryInner {
    entries: VecDeque<HistoryEntry>,
    capacity: usize,
}

impl HistoryStore {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HistoryInner {
                entries: VecDeque::with_capacity(capacity.min(1024)),
                capacity,
            })),
        }
    }

    pub fn insert(&self, flow: HttpFlow) {
        let mut inner = self.inner.write();
        if inner.entries.len() >= inner.capacity {
            inner.entries.pop_front();
        }
        inner.entries.push_back(HistoryEntry::from(flow));
    }

    pub fn update(&self, flow: HttpFlow) -> bool {
        let mut inner = self.inner.write();
        if let Some(entry) = inner.entries.iter_mut().find(|e| e.flow.id == flow.id) {
            entry.flow = flow;
            true
        } else {
            false
        }
    }

    pub fn get(&self, id: Uuid) -> Option<HistoryEntry> {
        self.inner
            .read()
            .entries
            .iter()
            .find(|e| e.flow.id == id)
            .cloned()
    }

    pub fn list(&self) -> Vec<HistoryEntry> {
        self.inner.read().entries.iter().rev().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.inner.read().entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().entries.is_empty()
    }

    pub fn clear(&self) {
        self.inner.write().entries.clear();
    }

    pub fn set_note(&self, id: Uuid, note: Option<String>) -> bool {
        let mut inner = self.inner.write();
        if let Some(entry) = inner.entries.iter_mut().find(|e| e.flow.id == id) {
            entry.note = note;
            true
        } else {
            false
        }
    }

    pub fn set_starred(&self, id: Uuid, starred: bool) -> bool {
        let mut inner = self.inner.write();
        if let Some(entry) = inner.entries.iter_mut().find(|e| e.flow.id == id) {
            entry.starred = starred;
            true
        } else {
            false
        }
    }

    pub fn search(&self, needle: &str) -> Vec<HistoryEntry> {
        let needle = needle.to_ascii_lowercase();
        self.inner
            .read()
            .entries
            .iter()
            .rev()
            .filter(|entry| {
                entry.flow.request.url.to_ascii_lowercase().contains(&needle)
                    || entry
                        .flow
                        .request
                        .method
                        .to_ascii_lowercase()
                        .contains(&needle)
                    || entry
                        .flow
                        .response
                        .as_ref()
                        .map(|r| r.status.to_string().contains(&needle))
                        .unwrap_or(false)
            })
            .cloned()
            .collect()
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;

    use super::*;
    use crate::model::{CapturedRequest, HeaderEntry};

    fn flow(url: &str) -> HttpFlow {
        let req = CapturedRequest {
            method: "GET".into(),
            url: url.into(),
            scheme: "https".into(),
            authority: "example.com".into(),
            path: "/".into(),
            http_version: "HTTP/1.1".into(),
            headers: vec![HeaderEntry::new("Host", "example.com")],
            body_b64: base64::engine::general_purpose::STANDARD.encode(b""),
            body_size: 0,
        };
        HttpFlow::new(req)
    }

    #[test]
    fn inserts_and_lists_in_reverse_order() {
        let store = HistoryStore::new();
        store.insert(flow("https://a.example.com/1"));
        store.insert(flow("https://b.example.com/2"));
        let list = store.list();
        assert_eq!(list.len(), 2);
        assert!(list[0].flow.request.url.contains("b."));
        assert!(list[1].flow.request.url.contains("a."));
    }

    #[test]
    fn evicts_when_at_capacity() {
        let store = HistoryStore::with_capacity(2);
        store.insert(flow("https://a/1"));
        store.insert(flow("https://b/2"));
        store.insert(flow("https://c/3"));
        assert_eq!(store.len(), 2);
        let urls: Vec<_> = store.list().iter().map(|e| e.flow.request.url.clone()).collect();
        assert!(urls.iter().any(|u| u.contains("/3")));
        assert!(urls.iter().any(|u| u.contains("/2")));
        assert!(!urls.iter().any(|u| u.contains("/1")));
    }

    #[test]
    fn search_filters_results() {
        let store = HistoryStore::new();
        store.insert(flow("https://api.example.com/login"));
        store.insert(flow("https://api.example.com/search"));
        let hits = store.search("login");
        assert_eq!(hits.len(), 1);
        assert!(hits[0].flow.request.url.contains("login"));
    }

    #[test]
    fn supports_notes_and_stars() {
        let store = HistoryStore::new();
        let f = flow("https://x/");
        let id = f.id;
        store.insert(f);
        assert!(store.set_note(id, Some("important".into())));
        assert!(store.set_starred(id, true));
        let entry = store.get(id).expect("entry exists");
        assert_eq!(entry.note.as_deref(), Some("important"));
        assert!(entry.starred);
    }
}
