//! Captured HTTP flows store. The flows live in-memory for fast lookup, but
//! [`HistoryStore::attach_file`] enables append-only JSON-Lines persistence
//! so that history survives an app restart. Each line of the file is one
//! serialised [`HistoryEntry`].

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use tracing::warn;
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
    persist: Arc<Mutex<Option<PathBuf>>>,
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
            persist: Arc::new(Mutex::new(None)),
        }
    }

    /// Replay any previously-persisted entries from disk, then start
    /// appending every future insert/update to the same file.
    pub fn attach_file(&self, path: impl AsRef<Path>) {
        let path = path.as_ref().to_path_buf();
        self.load_from_path(&path);
        *self.persist.lock() = Some(path);
    }

    fn load_from_path(&self, path: &Path) {
        let Ok(file) = File::open(path) else {
            return;
        };
        let reader = BufReader::new(file);
        let mut loaded: Vec<HistoryEntry> = Vec::new();
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<HistoryEntry>(&line) {
                Ok(entry) => loaded.push(entry),
                Err(err) => warn!(?err, "history: dropping malformed line"),
            }
        }
        let mut inner = self.inner.write();
        for entry in loaded {
            if inner.entries.len() >= inner.capacity {
                inner.entries.pop_front();
            }
            inner.entries.push_back(entry);
        }
    }

    fn rewrite_persist(&self) {
        let Some(path) = self.persist.lock().clone() else {
            return;
        };
        let snapshot: Vec<HistoryEntry> = self.inner.read().entries.iter().cloned().collect();
        let Ok(mut file) = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
        else {
            warn!(path = %path.display(), "history: failed to open for rewrite");
            return;
        };
        for entry in snapshot {
            match serde_json::to_string(&entry) {
                Ok(line) => {
                    if let Err(err) = writeln!(file, "{line}") {
                        warn!(?err, "history: failed to write line");
                        return;
                    }
                }
                Err(err) => warn!(?err, "history: failed to serialize entry"),
            }
        }
    }

    pub fn insert(&self, flow: HttpFlow) {
        let mut inner = self.inner.write();
        if inner.entries.len() >= inner.capacity {
            inner.entries.pop_front();
        }
        inner.entries.push_back(HistoryEntry::from(flow));
        drop(inner);
        // Append the new entry rather than rewriting the whole file, when
        // capacity is large enough that eviction isn't happening — but for
        // correctness we'll just rewrite. JSONL with ~5k entries is small
        // enough on modern disks (single-digit MBs).
        self.rewrite_persist();
    }

    pub fn update(&self, flow: HttpFlow) -> bool {
        let mut inner = self.inner.write();
        let ok = if let Some(entry) = inner.entries.iter_mut().find(|e| e.flow.id == flow.id) {
            entry.flow = flow;
            true
        } else {
            false
        };
        drop(inner);
        if ok {
            self.rewrite_persist();
        }
        ok
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
        self.rewrite_persist();
    }

    pub fn set_note(&self, id: Uuid, note: Option<String>) -> bool {
        let mut inner = self.inner.write();
        let ok = if let Some(entry) = inner.entries.iter_mut().find(|e| e.flow.id == id) {
            entry.note = note;
            true
        } else {
            false
        };
        drop(inner);
        if ok {
            self.rewrite_persist();
        }
        ok
    }

    pub fn set_starred(&self, id: Uuid, starred: bool) -> bool {
        let mut inner = self.inner.write();
        let ok = if let Some(entry) = inner.entries.iter_mut().find(|e| e.flow.id == id) {
            entry.starred = starred;
            true
        } else {
            false
        };
        drop(inner);
        if ok {
            self.rewrite_persist();
        }
        ok
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

    #[test]
    fn persists_and_reloads_from_disk() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("history.jsonl");

        // Phase 1: write three flows.
        let store = HistoryStore::new();
        store.attach_file(&path);
        store.insert(flow("https://api.example.com/one"));
        store.insert(flow("https://api.example.com/two"));
        let third = flow("https://api.example.com/three");
        let third_id = third.id;
        store.insert(third);
        assert!(store.set_starred(third_id, true));

        // Phase 2: a fresh store attached to the same file replays them.
        let reloaded = HistoryStore::new();
        reloaded.attach_file(&path);
        assert_eq!(reloaded.len(), 3);
        let entry = reloaded.get(third_id).expect("reloaded entry");
        assert!(entry.starred);
    }
}
