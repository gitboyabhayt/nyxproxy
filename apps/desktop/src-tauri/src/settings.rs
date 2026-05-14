//! Persisted user settings stored as JSON in the app data directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use nyxproxy_core::proxy::ProxyConfig;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Public, hosted NyxProxy AI gateway. The desktop app talks to this by
/// default so users do not need to spin up the FastAPI backend locally just to
/// use the AI features.
pub const DEFAULT_BACKEND_URL: &str = "https://nyxproxy-backend.onrender.com";

/// Legacy default that earlier builds shipped with. We migrate any persisted
/// `settings.json` away from this value so that updating the desktop binary is
/// enough to start talking to the hosted backend.
const LEGACY_LOCALHOST_BACKEND_URL: &str = "http://127.0.0.1:8765";

/// Resolve the backend URL to use for fresh installs. Build-time override via
/// `NYXPROXY_BACKEND_URL` lets self-hosters bake in their own URL without
/// shipping a patched binary.
pub fn default_backend_url() -> String {
    option_env!("NYXPROXY_BACKEND_URL")
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_BACKEND_URL.to_string())
}

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
            backend_url: default_backend_url(),
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
        let mut settings = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str::<Settings>(&raw).unwrap_or_default()
        } else {
            Settings::default()
        };
        migrate_settings(&mut settings);
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

/// Upgrade stored settings in place. Currently this only rewrites the legacy
/// localhost backend URL to the hosted Render URL, so users who first launched
/// an older build automatically pick up the live AI gateway.
fn migrate_settings(settings: &mut Settings) {
    let trimmed = settings.backend_url.trim();
    if trimmed.is_empty() || trimmed == LEGACY_LOCALHOST_BACKEND_URL {
        settings.backend_url = default_backend_url();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_localhost_url_migrates_to_default() {
        let mut s = Settings::default();
        s.backend_url = LEGACY_LOCALHOST_BACKEND_URL.into();
        migrate_settings(&mut s);
        assert_eq!(s.backend_url, default_backend_url());
    }

    #[test]
    fn empty_url_migrates_to_default() {
        let mut s = Settings::default();
        s.backend_url = "   ".into();
        migrate_settings(&mut s);
        assert_eq!(s.backend_url, default_backend_url());
    }

    #[test]
    fn custom_url_is_preserved() {
        let mut s = Settings::default();
        s.backend_url = "https://my-self-hosted-backend.example.com".into();
        let original = s.backend_url.clone();
        migrate_settings(&mut s);
        assert_eq!(s.backend_url, original);
    }
}
