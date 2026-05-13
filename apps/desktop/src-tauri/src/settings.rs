//! Persisted user settings stored as JSON in the app data directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use nyxproxy_core::proxy::ProxyConfig;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub proxy: ProxyConfig,
    pub backend_url: String,
    pub backend_token: Option<String>,
    pub default_ai_provider: String,
    pub theme: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            proxy: ProxyConfig::default(),
            backend_url: "http://127.0.0.1:8765".into(),
            backend_token: None,
            default_ai_provider: "groq".into(),
            theme: "dark".into(),
        }
    }
}

#[derive(Clone)]
pub struct SettingsStore {
    inner: Arc<RwLock<Settings>>,
    path: PathBuf,
}

impl SettingsStore {
    pub fn load(data_dir: &Path) -> Result<Self> {
        let path = data_dir.join("settings.json");
        let settings = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str::<Settings>(&raw).unwrap_or_default()
        } else {
            Settings::default()
        };
        let store = Self {
            inner: Arc::new(RwLock::new(settings)),
            path,
        };
        store.save_locked();
        Ok(store)
    }

    pub fn with<R>(&self, f: impl FnOnce(&Settings) -> R) -> R {
        f(&self.inner.read())
    }

    pub fn mutate(&self, f: impl FnOnce(&mut Settings)) {
        {
            let mut guard = self.inner.write();
            f(&mut guard);
        }
        self.save_locked();
    }

    pub fn replace(&self, new: Settings) {
        {
            let mut guard = self.inner.write();
            *guard = new;
        }
        self.save_locked();
    }

    fn save_locked(&self) {
        let snapshot = self.inner.read().clone();
        if let Err(err) = self.save_to_disk(&snapshot) {
            tracing::warn!(?err, "failed to persist settings");
        }
    }

    fn save_to_disk(&self, snapshot: &Settings) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialised = serde_json::to_string_pretty(snapshot)?;
        std::fs::write(&self.path, serialised)?;
        Ok(())
    }
}
