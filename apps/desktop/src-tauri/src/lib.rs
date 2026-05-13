//! Tauri shell for the NyxProxy desktop app.

use std::sync::Arc;

use parking_lot::Mutex;
use tauri::Manager;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod ai;
mod commands;
mod settings;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        // Already installed — that's fine.
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,nyxproxy_core=debug")))
        .init();

    let data_dir = directory_for_app();
    info!(path = %data_dir.display(), "using data directory");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(Mutex::new(None::<AppState>)))
        .setup(move |app| {
            let handle = app.handle().clone();
            let dir = data_dir.clone();
            tauri::async_runtime::spawn(async move {
                match AppState::initialise(handle.clone(), dir).await {
                    Ok(state) => {
                        let slot = handle.state::<Arc<Mutex<Option<AppState>>>>();
                        *slot.lock() = Some(state);
                        info!("nyxproxy initialised");
                    }
                    Err(err) => {
                        tracing::error!(?err, "failed to initialise app state");
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::proxy_status,
            commands::proxy_start,
            commands::proxy_stop,
            commands::proxy_get_config,
            commands::proxy_set_config,
            commands::intercept_list,
            commands::intercept_forward,
            commands::intercept_drop,
            commands::intercept_drop_all,
            commands::history_list,
            commands::history_get,
            commands::history_clear,
            commands::history_search,
            commands::history_set_note,
            commands::history_set_starred,
            commands::ca_info,
            commands::decoder_encode,
            commands::decoder_decode,
            commands::decoder_smart,
            commands::sequencer_analyze,
            commands::repeater_send,
            commands::intruder_run,
            commands::ai_chat,
            commands::ai_analyze_request,
            commands::ai_find_vulns,
            commands::ai_generate_payloads,
            commands::ai_list_providers,
            commands::scanner_scan_history,
            commands::scanner_scan_flow,
            commands::spider_run,
            commands::report_build,
            commands::report_render_html,
            commands::report_render_json,
            commands::plugins_list,
            commands::plugins_reload,
            commands::plugins_set_enabled,
            commands::plugins_scan_flow,
            commands::plugins_scan_history,
            commands::settings_get,
            commands::settings_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NyxProxy");
}

fn directory_for_app() -> std::path::PathBuf {
    if let Some(home) = dirs::home_dir() {
        let dir = home.join(".nyxproxy");
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }
    std::env::temp_dir().join("nyxproxy")
}
