//! WebSocket inspection — RFC 6455.
//!
//! When the MITM observes an HTTP/1.1 `Upgrade: websocket` exchange, the
//! tunnel is handed off to this module instead of being copied opaquely. We
//! parse every frame in both directions, surface them to the UI through
//! [`WsStore`], and allow the user to inject extra frames into either side
//! ("replay").
//!
//! The implementation is intentionally allocation-light: we read frame
//! headers in-place, take a single allocation for the unmasked payload, and
//! pass the bytes back to the upstream/downstream peer unmodified so the
//! man-in-the-middle is invisible to the application.

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::error::{NyxError, NyxResult};

/// Standard WebSocket opcodes from RFC 6455 §5.2.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WsOpcode {
    Continuation = 0x0,
    Text = 0x1,
    Binary = 0x2,
    Close = 0x8,
    Ping = 0x9,
    Pong = 0xA,
    Unknown = 0xF,
}

impl WsOpcode {
    pub fn from_u8(v: u8) -> Self {
        match v & 0x0f {
            0x0 => Self::Continuation,
            0x1 => Self::Text,
            0x2 => Self::Binary,
            0x8 => Self::Close,
            0x9 => Self::Ping,
            0xA => Self::Pong,
            _ => Self::Unknown,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn is_control(self) -> bool {
        matches!(self, Self::Close | Self::Ping | Self::Pong)
    }
}

/// Which peer sent the frame.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WsDirection {
    ClientToServer,
    ServerToClient,
}

/// A single captured frame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsFrame {
    pub id: Uuid,
    pub session_id: Uuid,
    pub direction: WsDirection,
    pub opcode: WsOpcode,
    pub fin: bool,
    pub masked: bool,
    /// Base64-encoded unmasked payload bytes (so binary frames survive JSON).
    pub payload_b64: String,
    pub payload_size: usize,
    /// Decoded UTF-8 for text frames. `None` for binary / unknown.
    pub text: Option<String>,
    pub captured_at: DateTime<Utc>,
    /// `true` when the frame originated from a `replay` call (i.e. injected
    /// by the user rather than seen on the wire).
    #[serde(default)]
    pub injected: bool,
}

impl WsFrame {
    pub fn from_payload(
        session_id: Uuid,
        direction: WsDirection,
        opcode: WsOpcode,
        fin: bool,
        masked: bool,
        payload: &[u8],
        injected: bool,
    ) -> Self {
        let payload_b64 = base64::engine::general_purpose::STANDARD.encode(payload);
        let text = if opcode == WsOpcode::Text {
            std::str::from_utf8(payload).ok().map(|s| s.to_string())
        } else {
            None
        };
        Self {
            id: Uuid::new_v4(),
            session_id,
            direction,
            opcode,
            fin,
            masked,
            payload_b64,
            payload_size: payload.len(),
            text,
            captured_at: Utc::now(),
            injected,
        }
    }

    pub fn payload_bytes(&self) -> Vec<u8> {
        base64::engine::general_purpose::STANDARD
            .decode(&self.payload_b64)
            .unwrap_or_default()
    }
}

/// A live or finished WebSocket session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsSession {
    pub id: Uuid,
    pub url: String,
    pub host: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub close_code: Option<u16>,
    pub close_reason: Option<String>,
    pub frame_count: usize,
}

impl WsSession {
    pub fn new(url: impl Into<String>, host: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            url: url.into(),
            host: host.into(),
            started_at: Utc::now(),
            ended_at: None,
            close_code: None,
            close_reason: None,
            frame_count: 0,
        }
    }
}

/// Events streamed from the WebSocket store to subscribers (Tauri → React).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WsEvent {
    SessionStarted { session: WsSession },
    Frame { frame: WsFrame },
    SessionEnded { session: WsSession },
}

/// Default ring-buffer size for stored sessions.
const DEFAULT_MAX_SESSIONS: usize = 200;
/// Default per-session frame ring-buffer size.
const DEFAULT_MAX_FRAMES_PER_SESSION: usize = 5_000;

#[derive(Debug)]
struct StoreInner {
    sessions: HashMap<Uuid, WsSession>,
    session_order: Vec<Uuid>,
    frames: HashMap<Uuid, Vec<WsFrame>>,
    /// One channel per active session for the proxy to publish *replay*
    /// frames back into the live stream. The sender is removed on
    /// `end_session`.
    replay_tx: HashMap<Uuid, mpsc::UnboundedSender<(WsDirection, WsOpcode, Vec<u8>)>>,
    max_sessions: usize,
    max_frames_per_session: usize,
}

/// Stores live and historical WebSocket sessions, broadcasts events to
/// subscribers, and exposes a *replay* path for user-injected frames.
#[derive(Clone)]
pub struct WsStore {
    inner: Arc<RwLock<StoreInner>>,
    events: broadcast::Sender<WsEvent>,
}

impl Default for WsStore {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_MAX_SESSIONS, DEFAULT_MAX_FRAMES_PER_SESSION)
    }
}

impl WsStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(max_sessions: usize, max_frames_per_session: usize) -> Self {
        let (events, _) = broadcast::channel(1024);
        Self {
            inner: Arc::new(RwLock::new(StoreInner {
                sessions: HashMap::new(),
                session_order: Vec::new(),
                frames: HashMap::new(),
                replay_tx: HashMap::new(),
                max_sessions,
                max_frames_per_session,
            })),
            events,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsEvent> {
        self.events.subscribe()
    }

    pub fn list_sessions(&self) -> Vec<WsSession> {
        let inner = self.inner.read();
        inner
            .session_order
            .iter()
            .rev()
            .filter_map(|id| inner.sessions.get(id).cloned())
            .collect()
    }

    pub fn get_session(&self, id: Uuid) -> Option<WsSession> {
        self.inner.read().sessions.get(&id).cloned()
    }

    pub fn frames_for(&self, id: Uuid) -> Vec<WsFrame> {
        self.inner
            .read()
            .frames
            .get(&id)
            .cloned()
            .unwrap_or_default()
    }

    /// Start a new session. The returned receiver yields user-injected
    /// frames that the proxy MUST forward to the remote endpoint.
    pub fn start_session(
        &self,
        url: String,
        host: String,
    ) -> (
        WsSession,
        mpsc::UnboundedReceiver<(WsDirection, WsOpcode, Vec<u8>)>,
    ) {
        let session = WsSession::new(url, host);
        let (tx, rx) = mpsc::unbounded_channel();
        {
            let mut inner = self.inner.write();
            inner.sessions.insert(session.id, session.clone());
            inner.session_order.push(session.id);
            inner.frames.insert(session.id, Vec::new());
            inner.replay_tx.insert(session.id, tx);

            // Evict the oldest if we're over capacity.
            while inner.session_order.len() > inner.max_sessions {
                let oldest = inner.session_order.remove(0);
                inner.sessions.remove(&oldest);
                inner.frames.remove(&oldest);
                inner.replay_tx.remove(&oldest);
            }
        }
        let _ = self.events.send(WsEvent::SessionStarted {
            session: session.clone(),
        });
        (session, rx)
    }

    pub fn end_session(
        &self,
        id: Uuid,
        close_code: Option<u16>,
        close_reason: Option<String>,
    ) {
        let session = {
            let mut inner = self.inner.write();
            inner.replay_tx.remove(&id);
            if let Some(s) = inner.sessions.get_mut(&id) {
                s.ended_at = Some(Utc::now());
                s.close_code = close_code;
                s.close_reason = close_reason;
                Some(s.clone())
            } else {
                None
            }
        };
        if let Some(session) = session {
            let _ = self.events.send(WsEvent::SessionEnded { session });
        }
    }

    pub fn push_frame(&self, frame: WsFrame) {
        {
            let mut inner = self.inner.write();
            let max = inner.max_frames_per_session;
            if let Some(buf) = inner.frames.get_mut(&frame.session_id) {
                buf.push(frame.clone());
                while buf.len() > max {
                    buf.remove(0);
                }
            }
            if let Some(s) = inner.sessions.get_mut(&frame.session_id) {
                s.frame_count = s.frame_count.saturating_add(1);
            }
        }
        let _ = self.events.send(WsEvent::Frame { frame });
    }

    /// Inject a frame as if it was sent from the user-controlled side.
    /// Returns an error if the session is already finished.
    pub fn replay(
        &self,
        session_id: Uuid,
        direction: WsDirection,
        opcode: WsOpcode,
        payload: Vec<u8>,
    ) -> NyxResult<()> {
        let tx = {
            let inner = self.inner.read();
            inner.replay_tx.get(&session_id).cloned()
        };
        let tx = tx.ok_or_else(|| NyxError::Proxy("websocket session not active".into()))?;
        tx.send((direction, opcode, payload))
            .map_err(|e| NyxError::Proxy(format!("replay send: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Frame parser / serialiser
// ---------------------------------------------------------------------------

/// A parsed (but possibly unfragmented) frame.
#[derive(Debug, Clone)]
pub struct ParsedFrame {
    pub fin: bool,
    pub opcode: WsOpcode,
    pub masked: bool,
    pub payload: Vec<u8>,
}

/// Read a single frame from an async byte stream.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> NyxResult<ParsedFrame> {
    let mut head = [0u8; 2];
    reader
        .read_exact(&mut head)
        .await
        .map_err(|e| NyxError::Proxy(format!("ws read head: {e}")))?;

    let fin = (head[0] & 0x80) != 0;
    let opcode = WsOpcode::from_u8(head[0]);
    let masked = (head[1] & 0x80) != 0;
    let len7 = head[1] & 0x7f;

    let payload_len: usize = match len7 {
        126 => {
            let mut b = [0u8; 2];
            reader
                .read_exact(&mut b)
                .await
                .map_err(|e| NyxError::Proxy(format!("ws read len16: {e}")))?;
            u16::from_be_bytes(b) as usize
        }
        127 => {
            let mut b = [0u8; 8];
            reader
                .read_exact(&mut b)
                .await
                .map_err(|e| NyxError::Proxy(format!("ws read len64: {e}")))?;
            u64::from_be_bytes(b) as usize
        }
        _ => len7 as usize,
    };

    let mask_key = if masked {
        let mut k = [0u8; 4];
        reader
            .read_exact(&mut k)
            .await
            .map_err(|e| NyxError::Proxy(format!("ws read mask: {e}")))?;
        Some(k)
    } else {
        None
    };

    let mut payload = vec![0u8; payload_len];
    if payload_len > 0 {
        reader
            .read_exact(&mut payload)
            .await
            .map_err(|e| NyxError::Proxy(format!("ws read payload: {e}")))?;
    }
    if let Some(k) = mask_key {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= k[i % 4];
        }
    }

    Ok(ParsedFrame {
        fin,
        opcode,
        masked,
        payload,
    })
}

/// Serialise a frame back onto the wire. Pass `mask: true` when sending from
/// client to server (per RFC 6455 §5.3).
pub fn encode_frame(frame: &ParsedFrame, mask: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(frame.payload.len() + 14);

    let b0 = if frame.fin { 0x80 } else { 0 } | (frame.opcode.as_u8() & 0x0f);
    out.push(b0);

    let len = frame.payload.len();
    let mask_bit: u8 = if mask { 0x80 } else { 0 };
    if len < 126 {
        out.push(mask_bit | (len as u8));
    } else if len < 65_536 {
        out.push(mask_bit | 126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(mask_bit | 127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }

    if mask {
        let key: [u8; 4] = rand::random();
        out.extend_from_slice(&key);
        let body_start = out.len();
        out.extend_from_slice(&frame.payload);
        for (i, b) in out[body_start..].iter_mut().enumerate() {
            *b ^= key[i % 4];
        }
    } else {
        out.extend_from_slice(&frame.payload);
    }

    out
}

/// Pump frames bidirectionally between the client (downstream) and upstream
/// server, capturing them into [`WsStore`] and injecting any replay frames
/// the user pushes via [`WsStore::replay`].
///
/// Returns when either peer closes the connection.
pub async fn proxy_pump<C, U>(
    store: WsStore,
    session: WsSession,
    mut replay_rx: mpsc::UnboundedReceiver<(WsDirection, WsOpcode, Vec<u8>)>,
    mut downstream: C,
    mut upstream: U,
) -> NyxResult<()>
where
    C: AsyncRead + AsyncWrite + Unpin,
    U: AsyncRead + AsyncWrite + Unpin,
{
    let session_id = session.id;
    let mut close_code: Option<u16> = None;
    let mut close_reason: Option<String> = None;

    let (mut down_r, mut down_w) = tokio::io::split(downstream_to_pair(&mut downstream));
    let (mut up_r, mut up_w) = tokio::io::split(upstream_to_pair(&mut upstream));

    loop {
        tokio::select! {
            biased;
            // Client → server
            res = read_frame(&mut down_r) => {
                let frame = match res { Ok(f) => f, Err(_) => break };
                let captured = WsFrame::from_payload(
                    session_id,
                    WsDirection::ClientToServer,
                    frame.opcode,
                    frame.fin,
                    frame.masked,
                    &frame.payload,
                    false,
                );
                store.push_frame(captured);
                if frame.opcode == WsOpcode::Close {
                    if frame.payload.len() >= 2 {
                        close_code = Some(u16::from_be_bytes([frame.payload[0], frame.payload[1]]));
                        if frame.payload.len() > 2 {
                            close_reason = std::str::from_utf8(&frame.payload[2..]).ok().map(|s| s.to_string());
                        }
                    }
                }
                let bytes = encode_frame(&frame, true);
                if up_w.write_all(&bytes).await.is_err() { break; }
                if frame.opcode == WsOpcode::Close { break; }
            }
            // Server → client
            res = read_frame(&mut up_r) => {
                let frame = match res { Ok(f) => f, Err(_) => break };
                let captured = WsFrame::from_payload(
                    session_id,
                    WsDirection::ServerToClient,
                    frame.opcode,
                    frame.fin,
                    frame.masked,
                    &frame.payload,
                    false,
                );
                store.push_frame(captured);
                if frame.opcode == WsOpcode::Close {
                    if frame.payload.len() >= 2 {
                        close_code = Some(u16::from_be_bytes([frame.payload[0], frame.payload[1]]));
                        if frame.payload.len() > 2 {
                            close_reason = std::str::from_utf8(&frame.payload[2..]).ok().map(|s| s.to_string());
                        }
                    }
                }
                let bytes = encode_frame(&frame, false);
                if down_w.write_all(&bytes).await.is_err() { break; }
                if frame.opcode == WsOpcode::Close { break; }
            }
            // User replay
            Some((dir, opcode, payload)) = replay_rx.recv() => {
                let frame = ParsedFrame { fin: true, opcode, masked: false, payload: payload.clone() };
                let injected = WsFrame::from_payload(session_id, dir, opcode, true, false, &payload, true);
                store.push_frame(injected);
                let (bytes, write_to_upstream) = match dir {
                    WsDirection::ClientToServer => (encode_frame(&frame, true), true),
                    WsDirection::ServerToClient => (encode_frame(&frame, false), false),
                };
                let res = if write_to_upstream {
                    up_w.write_all(&bytes).await
                } else {
                    down_w.write_all(&bytes).await
                };
                if res.is_err() { break; }
            }
        }
    }

    store.end_session(session_id, close_code, close_reason);
    Ok(())
}

fn downstream_to_pair<C: AsyncRead + AsyncWrite + Unpin>(c: &mut C) -> &mut C {
    c
}
fn upstream_to_pair<U: AsyncRead + AsyncWrite + Unpin>(u: &mut U) -> &mut U {
    u
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[test]
    fn opcode_round_trip() {
        for &op in &[
            WsOpcode::Continuation,
            WsOpcode::Text,
            WsOpcode::Binary,
            WsOpcode::Close,
            WsOpcode::Ping,
            WsOpcode::Pong,
        ] {
            assert_eq!(WsOpcode::from_u8(op.as_u8()), op);
        }
    }

    #[test]
    fn opcode_classifies_control() {
        assert!(WsOpcode::Close.is_control());
        assert!(WsOpcode::Ping.is_control());
        assert!(WsOpcode::Pong.is_control());
        assert!(!WsOpcode::Text.is_control());
        assert!(!WsOpcode::Binary.is_control());
    }

    #[test]
    fn encode_unmasked_short_text() {
        let frame = ParsedFrame {
            fin: true,
            opcode: WsOpcode::Text,
            masked: false,
            payload: b"hello".to_vec(),
        };
        let bytes = encode_frame(&frame, false);
        // 0x81 = FIN + text, 0x05 = unmasked len 5
        assert_eq!(bytes[..2], [0x81, 0x05]);
        assert_eq!(&bytes[2..], b"hello");
    }

    #[tokio::test]
    async fn encode_masked_short_text_round_trips_to_unmasked() {
        let frame = ParsedFrame {
            fin: true,
            opcode: WsOpcode::Text,
            masked: false,
            payload: b"hello".to_vec(),
        };
        let bytes = encode_frame(&frame, true);
        assert_eq!(bytes[0], 0x81);
        assert_eq!(bytes[1] & 0x80, 0x80, "mask bit set");
        assert_eq!(bytes[1] & 0x7f, 0x05, "len 5");
        let mut buf = std::io::Cursor::new(bytes);
        let parsed = read_frame(&mut buf).await.unwrap();
        assert!(parsed.fin);
        assert_eq!(parsed.opcode, WsOpcode::Text);
        assert!(parsed.masked);
        assert_eq!(parsed.payload, b"hello");
    }

    #[test]
    fn encode_len_16_boundary() {
        let frame = ParsedFrame {
            fin: true,
            opcode: WsOpcode::Binary,
            masked: false,
            payload: vec![0u8; 200],
        };
        let bytes = encode_frame(&frame, false);
        // 0x82 = FIN+bin, 0x7e = 126 → 16-bit ext len, then 0x00 0xc8 = 200
        assert_eq!(bytes[..4], [0x82, 0x7e, 0x00, 0xc8]);
    }

    #[test]
    fn encode_len_64_boundary() {
        let frame = ParsedFrame {
            fin: true,
            opcode: WsOpcode::Binary,
            masked: false,
            payload: vec![0u8; 70_000],
        };
        let bytes = encode_frame(&frame, false);
        assert_eq!(bytes[0], 0x82);
        assert_eq!(bytes[1] & 0x7f, 0x7f); // 127
    }

    #[test]
    fn store_records_session_and_frames() {
        let store = WsStore::new();
        let (session, _replay_rx) = store.start_session("wss://x.test/ws".into(), "x.test".into());
        store.push_frame(WsFrame::from_payload(
            session.id,
            WsDirection::ClientToServer,
            WsOpcode::Text,
            true,
            false,
            b"hi",
            false,
        ));
        store.push_frame(WsFrame::from_payload(
            session.id,
            WsDirection::ServerToClient,
            WsOpcode::Text,
            true,
            false,
            b"hi back",
            false,
        ));
        let frames = store.frames_for(session.id);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].text.as_deref(), Some("hi"));
        let s = store.get_session(session.id).unwrap();
        assert_eq!(s.frame_count, 2);
        store.end_session(session.id, Some(1000), Some("normal".into()));
        let s = store.get_session(session.id).unwrap();
        assert_eq!(s.close_code, Some(1000));
        assert!(s.ended_at.is_some());
    }

    #[test]
    fn store_caps_per_session_frames() {
        let store = WsStore::with_capacity(10, 3);
        let (session, _rx) = store.start_session("wss://x".into(), "x".into());
        for i in 0..10 {
            store.push_frame(WsFrame::from_payload(
                session.id,
                WsDirection::ClientToServer,
                WsOpcode::Text,
                true,
                false,
                format!("m{i}").as_bytes(),
                false,
            ));
        }
        let frames = store.frames_for(session.id);
        assert_eq!(frames.len(), 3, "ring buffer cap is 3");
        assert_eq!(frames.last().unwrap().text.as_deref(), Some("m9"));
    }

    #[test]
    fn store_evicts_oldest_sessions() {
        let store = WsStore::with_capacity(2, 100);
        let (s1, _r1) = store.start_session("a".into(), "a".into());
        let (_s2, _r2) = store.start_session("b".into(), "b".into());
        let (_s3, _r3) = store.start_session("c".into(), "c".into());
        let sessions = store.list_sessions();
        assert_eq!(sessions.len(), 2, "evicts oldest");
        assert!(store.get_session(s1.id).is_none(), "s1 evicted");
    }

    #[test]
    fn replay_fails_if_session_ended() {
        let store = WsStore::new();
        let (session, _rx) = store.start_session("a".into(), "a".into());
        store.end_session(session.id, None, None);
        let res = store.replay(
            session.id,
            WsDirection::ClientToServer,
            WsOpcode::Text,
            b"hi".to_vec(),
        );
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn proxy_pump_relays_text_frames_bidi() {
        let store = WsStore::new();
        let (session, replay_rx) =
            store.start_session("wss://e.test/ws".into(), "e.test".into());
        let session_id = session.id;
        let (mut client_outer, client_inner) = duplex(64 * 1024);
        let (upstream_inner, mut upstream_outer) = duplex(64 * 1024);

        let store_clone = store.clone();
        let pump = tokio::spawn(async move {
            let _ = proxy_pump(store_clone, session, replay_rx, client_inner, upstream_inner)
                .await;
        });

        // Client → server (masked)
        let f = ParsedFrame {
            fin: true,
            opcode: WsOpcode::Text,
            masked: false,
            payload: b"hello".to_vec(),
        };
        client_outer
            .write_all(&encode_frame(&f, true))
            .await
            .unwrap();
        let got = read_frame(&mut upstream_outer).await.unwrap();
        assert_eq!(got.payload, b"hello");
        assert_eq!(got.opcode, WsOpcode::Text);
        assert!(got.masked, "client \u{2192} server is masked");

        // Server → client (unmasked)
        let f2 = ParsedFrame {
            fin: true,
            opcode: WsOpcode::Text,
            masked: false,
            payload: b"world".to_vec(),
        };
        upstream_outer
            .write_all(&encode_frame(&f2, false))
            .await
            .unwrap();
        let got2 = read_frame(&mut client_outer).await.unwrap();
        assert_eq!(got2.payload, b"world");
        assert!(!got2.masked);

        // Close
        let close = ParsedFrame {
            fin: true,
            opcode: WsOpcode::Close,
            masked: false,
            payload: vec![0x03, 0xe8], // 1000 normal
        };
        client_outer
            .write_all(&encode_frame(&close, true))
            .await
            .unwrap();
        let _ = pump.await;

        let frames = store.frames_for(session_id);
        let session = store.get_session(session_id).unwrap();
        assert!(frames.len() >= 3, "captured at least 3 frames");
        assert_eq!(session.close_code, Some(1000));
        assert!(session.ended_at.is_some());
    }
}
