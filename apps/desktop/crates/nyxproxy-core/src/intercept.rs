//! Intercept queue — pause flows mid-flight so the user can inspect, edit,
//! forward or drop them. Mirrors Burp's "Intercept" tab.
//!
//! When the proxy has `intercept_enabled = true`, every request entering
//! [`crate::proxy::forward_capture`] is parked here before being sent
//! upstream. The Tauri commands surface forward / drop / modify operations
//! that resolve a oneshot channel held inside the [`InterceptSlot`].

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, oneshot};
use uuid::Uuid;

use crate::model::CapturedRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterceptKind {
    Request,
    // Response intercept is reserved for a later phase.
    Response,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptEntry {
    pub id: String,
    pub kind: InterceptKind,
    pub captured: CapturedRequest,
    /// Base64-encoded body — kept separate so the React layer can re-encode
    /// after edits without re-serialising the rest of the request.
    pub body_b64: String,
    pub enqueued_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InterceptUpdate {
    Enqueued {
        #[serde(flatten)]
        entry: InterceptEntry,
    },
    Resolved {
        id: String,
        decision: InterceptDecisionKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterceptDecisionKind {
    Forward,
    Drop,
}

pub enum InterceptDecision {
    Forward {
        request: CapturedRequest,
        body: Vec<u8>,
    },
    Drop,
}

struct Slot {
    entry: InterceptEntry,
    decision: oneshot::Sender<InterceptDecision>,
}

#[derive(Clone)]
pub struct InterceptQueue {
    inner: Arc<Mutex<HashMap<String, Slot>>>,
    events: broadcast::Sender<InterceptUpdate>,
}

impl Default for InterceptQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl InterceptQueue {
    pub fn new() -> Self {
        let (events, _rx) = broadcast::channel(256);
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            events,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<InterceptUpdate> {
        self.events.subscribe()
    }

    pub fn list(&self) -> Vec<InterceptEntry> {
        self.inner
            .lock()
            .values()
            .map(|s| s.entry.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    /// Block the calling future until the entry is resolved.
    pub async fn enqueue(
        &self,
        captured: CapturedRequest,
        body: Vec<u8>,
    ) -> InterceptDecision {
        let (tx, rx) = oneshot::channel();
        let id = Uuid::new_v4().to_string();
        let entry = InterceptEntry {
            id: id.clone(),
            kind: InterceptKind::Request,
            captured,
            body_b64: base64::engine::general_purpose::STANDARD.encode(&body),
            enqueued_at: chrono::Utc::now(),
        };
        self.inner.lock().insert(
            id.clone(),
            Slot {
                entry: entry.clone(),
                decision: tx,
            },
        );
        let _ = self.events.send(InterceptUpdate::Enqueued { entry });
        match rx.await {
            Ok(decision) => decision,
            // Sender dropped without sending — treat as drop.
            Err(_) => InterceptDecision::Drop,
        }
    }

    /// Resolve an entry by forwarding it. The caller may supply an edited
    /// request + body; pass `None`s to re-use the originally captured values.
    pub fn forward(
        &self,
        id: &str,
        edited_request: Option<CapturedRequest>,
        edited_body_b64: Option<String>,
    ) -> bool {
        let slot = self.inner.lock().remove(id);
        let Some(slot) = slot else { return false };
        let req = edited_request.unwrap_or(slot.entry.captured);
        let body = match edited_body_b64 {
            Some(b64) => base64::engine::general_purpose::STANDARD
                .decode(b64.as_bytes())
                .unwrap_or_default(),
            None => base64::engine::general_purpose::STANDARD
                .decode(slot.entry.body_b64.as_bytes())
                .unwrap_or_default(),
        };
        let _ = slot
            .decision
            .send(InterceptDecision::Forward { request: req, body });
        let _ = self.events.send(InterceptUpdate::Resolved {
            id: id.to_string(),
            decision: InterceptDecisionKind::Forward,
        });
        true
    }

    pub fn drop(&self, id: &str) -> bool {
        let slot = self.inner.lock().remove(id);
        let Some(slot) = slot else { return false };
        let _ = slot.decision.send(InterceptDecision::Drop);
        let _ = self.events.send(InterceptUpdate::Resolved {
            id: id.to_string(),
            decision: InterceptDecisionKind::Drop,
        });
        true
    }

    pub fn drop_all(&self) -> usize {
        let slots: Vec<(String, Slot)> = {
            let mut g = self.inner.lock();
            g.drain().collect()
        };
        let n = slots.len();
        for (id, slot) in slots {
            let _ = slot.decision.send(InterceptDecision::Drop);
            let _ = self.events.send(InterceptUpdate::Resolved {
                id,
                decision: InterceptDecisionKind::Drop,
            });
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::HeaderEntry;

    fn make_request() -> CapturedRequest {
        CapturedRequest {
            method: "GET".into(),
            url: "https://example.com/x".into(),
            scheme: "https".into(),
            authority: "example.com".into(),
            path: "/x".into(),
            http_version: "HTTP/1.1".into(),
            headers: vec![HeaderEntry::new("Host", "example.com")],
            body_b64: base64::engine::general_purpose::STANDARD.encode(b""),
            body_size: 0,
        }
    }

    #[tokio::test]
    async fn forward_returns_request_to_caller() {
        let q = InterceptQueue::new();
        let q2 = q.clone();
        let join = tokio::spawn(async move { q2.enqueue(make_request(), b"hello".to_vec()).await });
        // Spin until the entry appears.
        let mut id = None;
        for _ in 0..50 {
            if let Some(entry) = q.list().into_iter().next() {
                id = Some(entry.id);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let id = id.expect("entry should have been enqueued");
        assert!(q.forward(&id, None, None));
        let decision = join.await.unwrap();
        match decision {
            InterceptDecision::Forward { body, .. } => assert_eq!(body, b"hello"),
            InterceptDecision::Drop => panic!("expected forward"),
        }
    }

    #[tokio::test]
    async fn drop_signals_drop_decision() {
        let q = InterceptQueue::new();
        let q2 = q.clone();
        let join = tokio::spawn(async move { q2.enqueue(make_request(), Vec::new()).await });
        for _ in 0..50 {
            if let Some(entry) = q.list().into_iter().next() {
                assert!(q.drop(&entry.id));
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let decision = join.await.unwrap();
        matches!(decision, InterceptDecision::Drop);
    }

    #[tokio::test]
    async fn forward_can_replace_body() {
        let q = InterceptQueue::new();
        let q2 = q.clone();
        let join =
            tokio::spawn(async move { q2.enqueue(make_request(), b"orig".to_vec()).await });
        let mut id = None;
        for _ in 0..50 {
            if let Some(entry) = q.list().into_iter().next() {
                id = Some(entry.id);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        let id = id.unwrap();
        let new_b64 = base64::engine::general_purpose::STANDARD.encode(b"edited");
        assert!(q.forward(&id, None, Some(new_b64)));
        let decision = join.await.unwrap();
        match decision {
            InterceptDecision::Forward { body, .. } => assert_eq!(body, b"edited"),
            InterceptDecision::Drop => panic!(),
        }
    }
}
