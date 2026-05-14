//! Tauri command surface exposed to the React frontend.

use std::sync::Arc;

use futures_util::StreamExt;
use nyxproxy_core::decoder::{self, Codec, DecoderResult};
use nyxproxy_core::history::HistoryEntry;
use nyxproxy_core::intercept::InterceptEntry;
use nyxproxy_core::intruder::{run as run_intruder, IntruderAttempt, IntruderConfig};
use nyxproxy_core::jwt::{self, JwtBruteResult, JwtDecoded, JwtFinding};
use nyxproxy_core::macros::{run_macro, Macro, MacroRunResult};
use nyxproxy_core::model::{CapturedRequest, CapturedResponse};
use nyxproxy_core::owasp::{self, OwaspCategory};
use nyxproxy_core::plugins::PluginDescriptor;
use nyxproxy_core::proxy::ProxyConfig;
use nyxproxy_core::repeater::{self, RepeaterRequest};
use nyxproxy_core::report::{self, Report};
use nyxproxy_core::risk;
use nyxproxy_core::scanner::{self, Issue};
use nyxproxy_core::sequencer::{self, SequencerReport};
use nyxproxy_core::spider::{crawl as spider_crawl, SpiderConfig, SpiderHit};
use nyxproxy_core::workspace::{self, Workspace};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::ai::{
    AiClient, AnalyzeRequestBody, AnalyzeResponse, ChatRequest, ChatResponse,
    PayloadRequestBody, ProvidersResponse,
};
use crate::settings::Settings;
use crate::state::AppState;

type AppStateSlot = Arc<Mutex<Option<AppState>>>;

/// Helper to access the initialised state or return a friendly error.
fn with_state<R>(slot: &AppStateSlot, f: impl FnOnce(&AppState) -> R) -> Result<R, String> {
    let guard = slot.lock();
    let state = guard.as_ref().ok_or("nyxproxy is still initialising")?;
    Ok(f(state))
}

#[derive(Debug, Serialize)]
pub struct ProxyStatus {
    pub running: bool,
    pub listen_addr: String,
    pub history_size: usize,
    pub ca_cert_path: String,
}

#[tauri::command]
pub fn proxy_status(state: State<'_, AppStateSlot>) -> Result<ProxyStatus, String> {
    with_state(&state, |s| ProxyStatus {
        running: s.running(),
        listen_addr: s.proxy.snapshot_config().listen_addr,
        history_size: s.history.len(),
        ca_cert_path: s.ca.ca_cert_path().display().to_string(),
    })
}

#[tauri::command]
pub async fn proxy_start(state: State<'_, AppStateSlot>) -> Result<String, String> {
    let (proxy, handle_slot) = {
        let guard = state.lock();
        let app_state = guard.as_ref().ok_or("nyxproxy is still initialising")?;
        if app_state.running() {
            return Err("proxy already running".into());
        }
        (app_state.proxy.clone(), app_state.proxy_handle.clone())
    };
    let handle = proxy.bind().await.map_err(|e| e.to_string())?;
    let addr = handle.local_addr.to_string();
    *handle_slot.lock() = Some(handle);
    Ok(addr)
}

#[tauri::command]
pub async fn proxy_stop(state: State<'_, AppStateSlot>) -> Result<(), String> {
    let handle_slot = {
        let guard = state.lock();
        let app_state = guard.as_ref().ok_or("nyxproxy is still initialising")?;
        app_state.proxy_handle.clone()
    };
    let handle = handle_slot.lock().take();
    if let Some(h) = handle {
        h.shutdown();
        h.join().await;
    }
    Ok(())
}

#[tauri::command]
pub fn proxy_get_config(state: State<'_, AppStateSlot>) -> Result<ProxyConfig, String> {
    with_state(&state, |s| s.proxy.snapshot_config())
}

#[tauri::command]
pub fn proxy_set_config(
    state: State<'_, AppStateSlot>,
    config: ProxyConfig,
) -> Result<(), String> {
    with_state(&state, |s| s.update_proxy_config(config))
}

#[tauri::command]
pub fn intercept_list(state: State<'_, AppStateSlot>) -> Result<Vec<InterceptEntry>, String> {
    with_state(&state, |s| s.proxy.intercept.list())
}

#[derive(serde::Deserialize)]
pub struct InterceptForwardArgs {
    pub id: String,
    #[serde(default)]
    pub request: Option<CapturedRequest>,
    #[serde(default)]
    pub body_b64: Option<String>,
}

#[tauri::command]
pub fn intercept_forward(
    state: State<'_, AppStateSlot>,
    args: InterceptForwardArgs,
) -> Result<bool, String> {
    with_state(&state, |s| {
        s.proxy
            .intercept
            .forward(&args.id, args.request, args.body_b64)
    })
}

#[tauri::command]
pub fn intercept_drop(state: State<'_, AppStateSlot>, id: String) -> Result<bool, String> {
    with_state(&state, |s| s.proxy.intercept.drop(&id))
}

#[tauri::command]
pub fn intercept_drop_all(state: State<'_, AppStateSlot>) -> Result<usize, String> {
    with_state(&state, |s| s.proxy.intercept.drop_all())
}

#[tauri::command]
pub fn history_list(state: State<'_, AppStateSlot>) -> Result<Vec<HistoryEntry>, String> {
    with_state(&state, |s| s.history.list())
}

#[tauri::command]
pub fn history_get(
    state: State<'_, AppStateSlot>,
    id: Uuid,
) -> Result<Option<HistoryEntry>, String> {
    with_state(&state, |s| s.history.get(id))
}

#[tauri::command]
pub fn history_clear(state: State<'_, AppStateSlot>) -> Result<(), String> {
    with_state(&state, |s| s.history.clear())
}

#[tauri::command]
pub fn history_search(
    state: State<'_, AppStateSlot>,
    query: String,
) -> Result<Vec<HistoryEntry>, String> {
    with_state(&state, |s| s.history.search(&query))
}

#[tauri::command]
pub fn history_set_note(
    state: State<'_, AppStateSlot>,
    id: Uuid,
    note: Option<String>,
) -> Result<bool, String> {
    with_state(&state, |s| s.history.set_note(id, note))
}

#[tauri::command]
pub fn history_set_starred(
    state: State<'_, AppStateSlot>,
    id: Uuid,
    starred: bool,
) -> Result<bool, String> {
    with_state(&state, |s| s.history.set_starred(id, starred))
}

#[derive(Debug, Serialize)]
pub struct CaInfo {
    pub cert_pem: String,
    pub cert_path: String,
    pub data_dir: String,
}

#[tauri::command]
pub fn ca_info(state: State<'_, AppStateSlot>) -> Result<CaInfo, String> {
    with_state(&state, |s| CaInfo {
        cert_pem: s.ca.cert_pem().to_string(),
        cert_path: s.ca.ca_cert_path().display().to_string(),
        data_dir: s.ca.data_dir().display().to_string(),
    })
}

#[tauri::command]
pub fn decoder_encode(codec: Codec, input: String) -> Result<String, String> {
    decoder::encode(codec, &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn decoder_decode(codec: Codec, input: String) -> Result<String, String> {
    decoder::decode(codec, &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn decoder_smart(input: String) -> Result<Vec<DecoderResult>, String> {
    Ok(decoder::smart_decode(&input))
}

#[tauri::command]
pub fn sequencer_analyze(samples: Vec<String>) -> Result<SequencerReport, String> {
    Ok(sequencer::analyze(samples))
}

#[tauri::command]
pub async fn repeater_send(request: RepeaterRequest) -> Result<CapturedResponse, String> {
    repeater::send(&request).await.map_err(|e| e.to_string())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IntruderStartArgs {
    pub session_id: String,
    pub config: IntruderConfig,
}

#[tauri::command]
pub async fn intruder_run(
    app: AppHandle,
    args: IntruderStartArgs,
) -> Result<Vec<IntruderAttempt>, String> {
    let event_name = format!("nyxproxy://intruder/{}", args.session_id);
    let mut stream = Box::pin(run_intruder(&args.config));
    let mut collected = Vec::new();
    while let Some(attempt) = stream.next().await {
        if let Err(err) = app.emit(&event_name, &attempt) {
            tracing::warn!(?err, "intruder emit failed");
        }
        collected.push(attempt);
    }
    let _ = app.emit(&format!("{event_name}/done"), &args.session_id);
    Ok(collected)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatArgs {
    pub messages: Vec<crate::ai::ChatMessage>,
    pub provider: Option<String>,
    pub model: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_temperature() -> f64 {
    0.2
}

fn default_max_tokens() -> u32 {
    1024
}

fn ai_client_from_state(state: &AppState) -> AiClient {
    let s = state.current_settings();
    AiClient::new(s.backend_url, s.backend_token)
}

#[tauri::command]
pub async fn ai_chat(
    state: State<'_, AppStateSlot>,
    args: ChatArgs,
) -> Result<ChatResponse, String> {
    let client = with_state(&state, ai_client_from_state)?;
    client
        .chat(ChatRequest {
            messages: args.messages,
            provider: args.provider,
            model: args.model,
            temperature: args.temperature,
            max_tokens: args.max_tokens,
        })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_analyze_request(
    state: State<'_, AppStateSlot>,
    body: AnalyzeRequestBody,
) -> Result<AnalyzeResponse, String> {
    let client = with_state(&state, ai_client_from_state)?;
    client.analyze_request(body).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_find_vulns(
    state: State<'_, AppStateSlot>,
    body: AnalyzeRequestBody,
) -> Result<AnalyzeResponse, String> {
    let client = with_state(&state, ai_client_from_state)?;
    client.find_vulns(body).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_generate_payloads(
    state: State<'_, AppStateSlot>,
    body: PayloadRequestBody,
) -> Result<AnalyzeResponse, String> {
    let client = with_state(&state, ai_client_from_state)?;
    client.generate_payloads(body).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_list_providers(
    state: State<'_, AppStateSlot>,
) -> Result<ProvidersResponse, String> {
    let client = with_state(&state, ai_client_from_state)?;
    client.providers().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scanner_scan_history(state: State<'_, AppStateSlot>) -> Result<Vec<Issue>, String> {
    with_state(&state, |s| {
        let mut out = Vec::new();
        for entry in s.history.list() {
            out.extend(scanner::scan(&entry.flow));
        }
        out
    })
}

#[tauri::command]
pub fn scanner_scan_flow(
    state: State<'_, AppStateSlot>,
    flow_id: Uuid,
) -> Result<Vec<Issue>, String> {
    with_state(&state, |s| match s.history.get(flow_id) {
        Some(entry) => scanner::scan(&entry.flow),
        None => Vec::new(),
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpiderStartArgs {
    pub session_id: String,
    pub config: SpiderConfig,
}

#[tauri::command]
pub async fn spider_run(
    app: AppHandle,
    args: SpiderStartArgs,
) -> Result<Vec<SpiderHit>, String> {
    let event_name = format!("nyxproxy://spider/{}", args.session_id);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<SpiderHit>(64);
    let app_emit = app.clone();
    let event_for_task = event_name.clone();
    let emitter = tokio::spawn(async move {
        while let Some(hit) = rx.recv().await {
            if let Err(err) = app_emit.emit(&event_for_task, &hit) {
                tracing::warn!(?err, "spider emit failed");
            }
        }
    });
    let hits = spider_crawl(args.config, Some(tx)).await;
    let _ = emitter.await;
    let _ = app.emit(&format!("{event_name}/done"), &args.session_id);
    Ok(hits)
}

#[tauri::command]
pub fn report_build(state: State<'_, AppStateSlot>) -> Result<Report, String> {
    with_state(&state, |s| {
        let history = s.history.list();
        let mut issues = Vec::new();
        for entry in &history {
            issues.extend(scanner::scan(&entry.flow));
        }
        report::build(&history, &issues)
    })
}

#[tauri::command]
pub fn report_render_html(report: Report) -> Result<String, String> {
    Ok(report::render_html(&report))
}

#[tauri::command]
pub fn report_render_json(report: Report) -> Result<String, String> {
    Ok(report::render_json(&report))
}

#[tauri::command]
pub fn plugins_list(state: State<'_, AppStateSlot>) -> Result<Vec<PluginDescriptor>, String> {
    with_state(&state, |s| s.plugins.list())
}

#[tauri::command]
pub fn plugins_reload(state: State<'_, AppStateSlot>) -> Result<Vec<PluginDescriptor>, String> {
    let mgr = with_state(&state, |s| s.plugins.clone())?;
    mgr.reload().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn plugins_set_enabled(
    state: State<'_, AppStateSlot>,
    id: String,
    enabled: bool,
) -> Result<bool, String> {
    with_state(&state, |s| s.plugins.set_enabled(&id, enabled))
}

#[tauri::command]
pub async fn plugins_scan_flow(
    state: State<'_, AppStateSlot>,
    id: String,
    flow_id: Uuid,
) -> Result<Vec<Issue>, String> {
    let (mgr, flow) = with_state(&state, |s| {
        (s.plugins.clone(), s.history.get(flow_id).map(|e| e.flow))
    })?;
    let Some(flow) = flow else {
        return Err(format!("flow {flow_id} not found"));
    };
    mgr.scan_flow(&id, &flow).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn plugins_scan_history(
    state: State<'_, AppStateSlot>,
) -> Result<Vec<Issue>, String> {
    let (mgr, flows) = with_state(&state, |s| {
        let flows: Vec<_> = s.history.list().into_iter().map(|e| e.flow).collect();
        (s.plugins.clone(), flows)
    })?;
    let mut out = Vec::new();
    for flow in flows {
        out.extend(mgr.scan_flow_all(&flow).await);
    }
    Ok(out)
}

#[tauri::command]
pub fn macros_list(state: State<'_, AppStateSlot>) -> Result<Vec<Macro>, String> {
    with_state(&state, |s| s.macros.list())
}

#[tauri::command]
pub fn macros_save(state: State<'_, AppStateSlot>, macro_: Macro) -> Result<Macro, String> {
    let store = with_state(&state, |s| s.macros.clone())?;
    store.save(macro_).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn macros_delete(state: State<'_, AppStateSlot>, id: String) -> Result<bool, String> {
    let store = with_state(&state, |s| s.macros.clone())?;
    store.delete(&id).map_err(|e| e.to_string())
}

#[derive(serde::Deserialize)]
pub struct MacroRunArgs {
    pub id: String,
    #[serde(default)]
    pub variables: std::collections::HashMap<String, String>,
}

#[tauri::command]
pub async fn macros_run(
    state: State<'_, AppStateSlot>,
    args: MacroRunArgs,
) -> Result<MacroRunResult, String> {
    let mac = with_state(&state, |s| s.macros.get(&args.id))?;
    let Some(mac) = mac else {
        return Err(format!("macro {} not found", args.id));
    };
    Ok(run_macro(&mac, args.variables).await)
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppStateSlot>) -> Result<Settings, String> {
    with_state(&state, |s| s.current_settings())
}

#[tauri::command]
pub fn settings_set(
    state: State<'_, AppStateSlot>,
    settings: Settings,
) -> Result<(), String> {
    with_state(&state, |s| s.settings.replace(settings))
}

// ---------------------------------------------------------------------------
// JWT toolkit
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn jwt_decode_cmd(token: String) -> Result<JwtDecoded, String> {
    jwt::decode(&token).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn jwt_analyze_cmd(token: String) -> Result<Vec<JwtFinding>, String> {
    jwt::analyze(&token).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
pub struct JwtEncodeArgs {
    pub header: serde_json::Value,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub secret: String,
}

#[tauri::command]
pub fn jwt_encode_hs256_cmd(args: JwtEncodeArgs) -> Result<String, String> {
    jwt::encode_hs256(&args.header, &args.payload, args.secret.as_bytes())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn jwt_encode_none_cmd(args: JwtEncodeArgs) -> Result<String, String> {
    jwt::encode_none(&args.header, &args.payload).map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
pub struct JwtBruteArgs {
    pub token: String,
    pub candidates: Vec<String>,
}

#[tauri::command]
pub fn jwt_brute_hs256_cmd(args: JwtBruteArgs) -> Result<JwtBruteResult, String> {
    jwt::brute_hs256(&args.token, &args.candidates).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Risk score / OWASP enrichment
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct IssueRisk {
    pub rule_id: String,
    pub score: u8,
    pub owasp_code: &'static str,
    pub owasp_title: &'static str,
}

#[tauri::command]
pub fn risk_score_issue_cmd(issue: Issue) -> Result<IssueRisk, String> {
    let cat = owasp::category_for_rule(&issue.rule_id);
    Ok(IssueRisk {
        rule_id: issue.rule_id.clone(),
        score: risk::score_issue(&issue),
        owasp_code: cat.code(),
        owasp_title: cat.title(),
    })
}

#[derive(Debug, Serialize)]
pub struct RiskSummary {
    pub aggregate: u8,
    pub by_owasp: Vec<OwaspBucket>,
}

#[derive(Debug, Serialize)]
pub struct OwaspBucket {
    pub code: &'static str,
    pub title: &'static str,
    pub count: usize,
    pub max_score: u8,
}

#[tauri::command]
pub fn risk_summary_cmd(issues: Vec<Issue>) -> Result<RiskSummary, String> {
    let aggregate = risk::score_aggregate(&issues);
    let mut buckets: std::collections::BTreeMap<&'static str, OwaspBucket> =
        std::collections::BTreeMap::new();
    for issue in &issues {
        let cat = owasp::category_for_rule(&issue.rule_id);
        let score = risk::score_issue(issue);
        let entry = buckets.entry(cat.code()).or_insert(OwaspBucket {
            code: cat.code(),
            title: cat.title(),
            count: 0,
            max_score: 0,
        });
        entry.count += 1;
        if score > entry.max_score {
            entry.max_score = score;
        }
    }
    Ok(RiskSummary {
        aggregate,
        by_owasp: buckets.into_values().collect(),
    })
}

// ---------------------------------------------------------------------------
// Workspace save / load
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WorkspaceSaveArgs {
    pub path: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub scope: Vec<String>,
}

#[tauri::command]
pub fn workspace_save_cmd(
    state: State<'_, AppStateSlot>,
    args: WorkspaceSaveArgs,
) -> Result<String, String> {
    let (history, issues) = with_state(&state, |s| {
        let history = s.history.list();
        let issues: Vec<Issue> = history.iter().flat_map(|e| scanner::scan(&e.flow)).collect();
        (history, issues)
    })?;
    let mut workspace = Workspace {
        name: args.name,
        notes: args.notes,
        scope: args.scope,
        history,
        issues,
        ..Default::default()
    };
    workspace.touch(env!("CARGO_PKG_VERSION"));
    let path = std::path::PathBuf::from(&args.path);
    workspace::save_to_path(&path, &workspace).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn workspace_load_cmd(path: String) -> Result<Workspace, String> {
    let path = std::path::PathBuf::from(path);
    workspace::load_from_path(&path).map_err(|e| e.to_string())
}

// silence "unused" lint when OwaspCategory is only used transitively
#[allow(dead_code)]
fn _owasp_keepalive() -> OwaspCategory {
    OwaspCategory::Other
}
