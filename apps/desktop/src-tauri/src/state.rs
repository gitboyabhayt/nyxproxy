//! Process-global state for the Tauri shell.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use nyxproxy_core::ca::CertAuthority;
use nyxproxy_core::history::HistoryStore;
use nyxproxy_core::proxy::{Proxy, ProxyConfig, ProxyHandle};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};

use crate::settings::{Settings, SettingsStore};

pub struct AppState {
    #[allow(dead_code)]
    pub data_dir: PathBuf,
    pub ca: CertAuthority,
    pub history: HistoryStore,
    pub proxy: Proxy,
    pub proxy_handle: Arc<Mutex<Option<ProxyHandle>>>,
    pub settings: SettingsStore,
}

impl AppState {
    pub async fn initialise(handle: AppHandle, data_dir: PathBuf) -> Result<Self> {
        let ca = CertAuthority::load_or_generate(&data_dir)?;
        let history = HistoryStore::new();
        history.attach_file(data_dir.join("history.jsonl"));
        let settings = SettingsStore::load(&data_dir)?;

        let config = settings.with(|s| s.proxy.clone());
        let proxy = Proxy::new(ca.clone(), history.clone(), config);

        // Fan proxy events out to the Tauri webview.
        let mut rx = proxy.subscribe();
        let app = handle.clone();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                if let Err(err) = app.emit("nyxproxy://proxy", &event) {
                    tracing::warn!(?err, "failed to emit proxy event");
                }
            }
        });

        // Fan intercept queue updates out as well so the React layer can keep
        // its hold/forward/drop view live.
        let mut irx = proxy.intercept.subscribe();
        let app2 = handle.clone();
        tokio::spawn(async move {
            while let Ok(update) = irx.recv().await {
                if let Err(err) = app2.emit("nyxproxy://intercept", &update) {
                    tracing::warn!(?err, "failed to emit intercept event");
                }
            }
        });

        Ok(Self {
            data_dir,
            ca,
            history,
            proxy,
            proxy_handle: Arc::new(Mutex::new(None)),
            settings,
        })
    }

    pub fn running(&self) -> bool {
        self.proxy_handle.lock().is_some()
    }

    pub fn current_settings(&self) -> Settings {
        self.settings.with(|s| s.clone())
    }

    pub fn update_proxy_config(&self, cfg: ProxyConfig) {
        self.proxy.update_config(cfg.clone());
        self.settings.mutate(|s| s.proxy = cfg);
    }
}
