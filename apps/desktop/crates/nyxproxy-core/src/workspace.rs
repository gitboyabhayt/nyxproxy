//! Project workspace format — save and load the full session state to a
//! single portable `.nyxproxy` file so users can hand projects between
//! machines or archive engagements.
//!
//! The on-disk format is **zstd-compressed JSON** with a tiny header for
//! versioning and tamper detection:
//!
//! ```text
//! [0..6]   magic = b"NYXPRJ"
//! [6..8]   format version (u16 LE)
//! [8..]    zstd-compressed JSON body of `Workspace`
//! ```
//!
//! Keeping the body as JSON (rather than raw bincode or tar) makes it trivial
//! to inspect old workspace files with `zstd -d | jq` and survives schema
//! drift via tolerant deserialisation.

use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{NyxError, NyxResult};
use crate::history::HistoryEntry;
use crate::scanner::Issue;

const MAGIC: &[u8; 6] = b"NYXPRJ";
const FORMAT_VERSION: u16 = 1;

/// Snapshot of one engagement.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Workspace {
    /// Human-readable label.
    #[serde(default)]
    pub name: String,
    /// Free-form notes from the analyst.
    #[serde(default)]
    pub notes: String,
    /// Whitelisted hosts / scope.
    #[serde(default)]
    pub scope: Vec<String>,
    /// Captured flows.
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    /// Findings.
    #[serde(default)]
    pub issues: Vec<Issue>,
    /// ISO-8601 timestamp the workspace was written.
    #[serde(default)]
    pub saved_at: String,
    /// NyxProxy version that produced the file.
    #[serde(default)]
    pub app_version: String,
}

impl Workspace {
    pub fn touch(&mut self, app_version: impl Into<String>) {
        self.saved_at = chrono::Utc::now().to_rfc3339();
        self.app_version = app_version.into();
    }
}

/// Serialize `workspace` to disk at `path`. Overwrites if the file exists.
pub fn save_to_path(path: &Path, workspace: &Workspace) -> NyxResult<()> {
    let bytes = serialize(workspace)?;
    std::fs::write(path, bytes).map_err(NyxError::from)
}

/// Read a workspace from `path`.
pub fn load_from_path(path: &Path) -> NyxResult<Workspace> {
    let bytes = std::fs::read(path).map_err(NyxError::from)?;
    deserialize(&bytes)
}

/// Serialize a workspace to the framed `.nyxproxy` byte form.
pub fn serialize(workspace: &Workspace) -> NyxResult<Vec<u8>> {
    let json = serde_json::to_vec(workspace)
        .map_err(|e| NyxError::Decode(format!("workspace json: {e}")))?;
    let mut encoder = zstd::Encoder::new(Vec::new(), 3)
        .map_err(|e| NyxError::Internal(format!("zstd init: {e}")))?;
    encoder
        .write_all(&json)
        .map_err(|e| NyxError::Internal(format!("zstd write: {e}")))?;
    let compressed = encoder
        .finish()
        .map_err(|e| NyxError::Internal(format!("zstd finish: {e}")))?;

    let mut out = Vec::with_capacity(8 + compressed.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(out)
}

/// Decode a framed `.nyxproxy` byte string back into a [`Workspace`].
pub fn deserialize(bytes: &[u8]) -> NyxResult<Workspace> {
    if bytes.len() < 8 {
        return Err(NyxError::Decode("workspace: file too short".into()));
    }
    if &bytes[0..6] != MAGIC {
        return Err(NyxError::Decode(
            "workspace: bad magic — not a .nyxproxy file".into(),
        ));
    }
    let version = u16::from_le_bytes([bytes[6], bytes[7]]);
    if version > FORMAT_VERSION {
        return Err(NyxError::Decode(format!(
            "workspace: format v{version} is newer than supported v{FORMAT_VERSION}"
        )));
    }
    let mut decoder = zstd::Decoder::new(&bytes[8..])
        .map_err(|e| NyxError::Internal(format!("zstd init: {e}")))?;
    let mut json = Vec::new();
    decoder
        .read_to_end(&mut json)
        .map_err(|e| NyxError::Internal(format!("zstd read: {e}")))?;

    let workspace: Workspace = serde_json::from_slice(&json)
        .map_err(|e| NyxError::Decode(format!("workspace json: {e}")))?;
    Ok(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Workspace {
        Workspace {
            name: "demo".into(),
            notes: "engagement notes".into(),
            scope: vec!["example.com".into(), "api.example.com".into()],
            history: Vec::new(),
            issues: Vec::new(),
            saved_at: "2026-01-01T00:00:00Z".into(),
            app_version: "0.1.0".into(),
        }
    }

    #[test]
    fn round_trip_preserves_content() {
        let w = sample();
        let bytes = serialize(&w).expect("serialize");
        let back = deserialize(&bytes).expect("deserialize");
        assert_eq!(back.name, w.name);
        assert_eq!(back.notes, w.notes);
        assert_eq!(back.scope, w.scope);
    }

    #[test]
    fn rejects_unknown_magic() {
        let mut bad = b"NOPROJ".to_vec();
        bad.extend_from_slice(&1u16.to_le_bytes());
        bad.extend_from_slice(b"trash");
        assert!(deserialize(&bad).is_err());
    }

    #[test]
    fn rejects_short_input() {
        assert!(deserialize(&[1, 2, 3]).is_err());
    }

    #[test]
    fn save_and_load_via_tempfile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.nyxproxy");
        let w = sample();
        save_to_path(&path, &w).expect("save");
        let back = load_from_path(&path).expect("load");
        assert_eq!(back.scope, w.scope);
    }
}
