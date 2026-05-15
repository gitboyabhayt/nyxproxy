//! Process-global state for the Tauri shell.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use nyxproxy_core::bridge::{self, BridgeConfig};
use nyxproxy_core::ca::CertAuthority;
use nyxproxy_core::history::HistoryStore;
use nyxproxy_core::macros::MacroStore;
use nyxproxy_core::monitor::MonitorState;
use nyxproxy_core::playwright::PlaywrightStore;
use nyxproxy_core::plugins::PluginManager;
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
    pub plugins: PluginManager,
    pub macros: MacroStore,
    pub playwright: PlaywrightStore,
    pub monitor: Arc<Mutex<MonitorState>>,
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

        // Fan WebSocket session/frame events to the webview.
        let mut wrx = proxy.ws_store.subscribe();
        let app3 = handle.clone();
        tokio::spawn(async move {
            while let Ok(event) = wrx.recv().await {
                if let Err(err) = app3.emit("nyxproxy://websocket", &event) {
                    tracing::warn!(?err, "failed to emit websocket event");
                }
            }
        });

        let plugins_dir = data_dir.join("plugins");
        let plugins = PluginManager::new(&plugins_dir);
        if let Err(err) = plugins.reload() {
            tracing::warn!(?err, "plugins: initial load failed");
        }

        let macros = MacroStore::open(data_dir.join("macros.json"))?;
        let playwright = PlaywrightStore::open(data_dir.join("playwright"))?;

        // Continuous monitoring: load any persisted schedules if present.
        let monitor_path = data_dir.join("monitor.json");
        let monitor_initial = if monitor_path.exists() {
            match std::fs::read(&monitor_path) {
                Ok(bytes) => serde_json::from_slice::<MonitorState>(&bytes).unwrap_or_default(),
                Err(_) => MonitorState::default(),
            }
        } else {
            MonitorState::default()
        };
        let monitor = Arc::new(Mutex::new(monitor_initial));

        // Bridge server: local-only HTTP API for the browser extension and CI.
        // Failure to bind (e.g. another instance already running) is logged
        // but not fatal — the desktop app stays usable without the bridge.
        let bridge_cfg = BridgeConfig::default();
        match bridge::start(bridge_cfg, history.clone()).await {
            Ok(h) => {
                tracing::info!(addr = %h.local_addr, "bridge listening");
                let _ = h.task;
            }
            Err(err) => {
                tracing::warn!(?err, "bridge: failed to start (continuing without it)");
            }
        }

        Ok(Self {
            data_dir,
            ca,
            history,
            proxy,
            proxy_handle: Arc::new(Mutex::new(None)),
            settings,
            plugins,
            macros,
            playwright,
            monitor,
        })
    }

    pub fn persist_monitor(&self) {
        let path = self.data_dir.join("monitor.json");
        let snapshot = self.monitor.lock().clone();
        if let Ok(bytes) = serde_json::to_vec_pretty(&snapshot) {
            let _ = std::fs::write(path, bytes);
        }
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
