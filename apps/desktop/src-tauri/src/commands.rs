//! Tauri command surface exposed to the React frontend.

use std::sync::Arc;

use futures_util::StreamExt;
use nyxproxy_core::burp_import::{self, BurpImportSummary};
use nyxproxy_core::compliance::{self, ComplianceFramework, ComplianceReport};
use nyxproxy_core::decoder::{self, Codec, DecoderResult};
use nyxproxy_core::graphql::{self, GraphQLAttackCase, GraphQLSchema};
use nyxproxy_core::pcap;
use nyxproxy_core::history::HistoryEntry;
use nyxproxy_core::intercept::InterceptEntry;
use nyxproxy_core::intruder::{run as run_intruder, IntruderAttempt, IntruderConfig};
use nyxproxy_core::jwt::{self, JwtBruteResult, JwtDecoded, JwtFinding};
use nyxproxy_core::macros::{run_macro, Macro, MacroRunResult};
use nyxproxy_core::model::{CapturedRequest, CapturedResponse};
use nyxproxy_core::monitor::{Cadence, MonitorRunRecord, MonitorSchedule};
use nyxproxy_core::nyxshare::{self, ShareManifest, SharePayload};
use nyxproxy_core::openapi::{self, OpenApiPlan};
use nyxproxy_core::owasp::{self, OwaspCategory};
use nyxproxy_core::owasp_dashboard::{self, OwaspDashboard};
use nyxproxy_core::selfhost::{self, SelfHostBundle, SelfHostConfig};
use nyxproxy_core::plugins::PluginDescriptor;
use nyxproxy_core::proxy::ProxyConfig;
use nyxproxy_core::repeater::{self, RepeaterRequest};
use nyxproxy_core::report::{self, Report};
use nyxproxy_core::risk;
use nyxproxy_core::scanner::{self, Issue};
use nyxproxy_core::sequencer::{self, SequencerReport};
use nyxproxy_core::spider::{crawl as spider_crawl, SpiderConfig, SpiderHit};
use nyxproxy_core::websocket::{WsDirection, WsFrame, WsOpcode, WsSession};
use nyxproxy_core::workspace::{self, Workspace};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

use crate::ai::{
    AiClient, AnalyzeRequestBody, AnalyzeResponse, AutoAttackPlan, AutoAttackRequestBody,
    ChainScanRequestBody, ChainScanResponse, ChatRequest, ChatResponse, FuzzMutateRequestBody,
    FuzzMutateResponse, PayloadRequestBody, ProvidersResponse,
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
pub async fn ai_auto_attack(
    state: State<'_, AppStateSlot>,
    body: AutoAttackRequestBody,
) -> Result<AutoAttackPlan, String> {
    let client = with_state(&state, ai_client_from_state)?;
    client.auto_attack(body).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_fuzz_mutate(
    state: State<'_, AppStateSlot>,
    body: FuzzMutateRequestBody,
) -> Result<FuzzMutateResponse, String> {
    let client = with_state(&state, ai_client_from_state)?;
    client.fuzz_mutate(body).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ai_chain_scan(
    state: State<'_, AppStateSlot>,
    body: ChainScanRequestBody,
) -> Result<ChainScanResponse, String> {
    let client = with_state(&state, ai_client_from_state)?;
    client.chain_scan(body).await.map_err(|e| e.to_string())
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

// ---------------------------------------------------------------------------
// Burp Suite XML import (Feature E)
// ---------------------------------------------------------------------------

/// Import a Burp Suite "Save items" XML export from disk. Reads the file at
/// `path`, parses every `<item>`, and appends each request/response pair into
/// the live [`HistoryStore`] tagged with `import:burp`.
///
/// Returns a [`BurpImportSummary`] describing how many items were seen,
/// imported, and skipped, plus the Burp version embedded in the file.
#[tauri::command]
pub fn burp_import_cmd(
    state: State<'_, AppStateSlot>,
    path: String,
) -> Result<BurpImportSummary, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?;
    let (flows, summary) =
        burp_import::parse_burp_xml(&bytes).map_err(|e| format!("parse burp xml: {e}"))?;
    with_state(&state, |s| {
        for flow in flows {
            s.history.insert(flow);
        }
    })?;
    Ok(summary)
}

// ---------------------------------------------------------------------------
// OpenAPI / Swagger auto-test generator (Feature BB)
// ---------------------------------------------------------------------------

/// Parse an OpenAPI / Swagger document and return an [`OpenApiPlan`] of
/// generated auth-bypass / IDOR / rate-limit test cases.
///
/// `path` is read from disk. `base_override` lets the caller substitute the
/// spec's `servers[0].url` (or Swagger 2 host+basePath) so the same spec can
/// be aimed at staging vs production without modification.
#[tauri::command]
pub fn openapi_build_plan_cmd(
    path: String,
    base_override: Option<String>,
) -> Result<OpenApiPlan, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("read {path}: {e}"))?;
    openapi::build_plan(&bytes, base_override.as_deref()).map_err(|e| e.to_string())
}

// silence "unused" lint when OwaspCategory is only used transitively
#[allow(dead_code)]
fn _owasp_keepalive() -> OwaspCategory {
    OwaspCategory::Other
}

// ---------------------------------------------------------------------------
// WebSocket viewer (Feature A)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn ws_list_sessions(state: State<'_, AppStateSlot>) -> Result<Vec<WsSession>, String> {
    with_state(&state, |s| s.proxy.ws_store.list_sessions())
}

#[tauri::command]
pub fn ws_get_session(
    state: State<'_, AppStateSlot>,
    id: String,
) -> Result<Option<WsSession>, String> {
    let id = Uuid::parse_str(&id).map_err(|e| format!("bad session id: {e}"))?;
    with_state(&state, |s| s.proxy.ws_store.get_session(id))
}

#[tauri::command]
pub fn ws_frames(
    state: State<'_, AppStateSlot>,
    session_id: String,
) -> Result<Vec<WsFrame>, String> {
    let id = Uuid::parse_str(&session_id).map_err(|e| format!("bad session id: {e}"))?;
    with_state(&state, |s| s.proxy.ws_store.frames_for(id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WsReplayArgs {
    pub session_id: String,
    pub direction: WsDirection,
    pub opcode: WsOpcode,
    /// Base64-encoded payload. Empty string is permitted (e.g. ping).
    pub payload_b64: Option<String>,
    /// UTF-8 text payload. Mutually exclusive with `payload_b64`.
    pub text: Option<String>,
}

#[tauri::command]
pub fn ws_replay(state: State<'_, AppStateSlot>, args: WsReplayArgs) -> Result<(), String> {
    use base64::Engine;
    let id = Uuid::parse_str(&args.session_id).map_err(|e| format!("bad session id: {e}"))?;
    let bytes = if let Some(b64) = args.payload_b64 {
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| format!("invalid base64: {e}"))?
    } else if let Some(text) = args.text {
        text.into_bytes()
    } else {
        Vec::new()
    };
    with_state(&state, |s| {
        s.proxy
            .ws_store
            .replay(id, args.direction, args.opcode, bytes)
    })?
    .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// GraphQL native (Feature R)
// ---------------------------------------------------------------------------

/// Return the list of history flows that look like GraphQL requests.
#[tauri::command]
pub fn graphql_list_endpoints(state: State<'_, AppStateSlot>) -> Result<Vec<String>, String> {
    with_state(&state, |s| {
        let mut urls: Vec<String> = s
            .history
            .list()
            .into_iter()
            .filter(|e| graphql::is_graphql_request(&e.flow))
            .map(|e| e.flow.request.url.clone())
            .collect();
        urls.sort();
        urls.dedup();
        urls
    })
}

/// Return the canonical introspection query string.
#[tauri::command]
pub fn graphql_introspection_query() -> Result<String, String> {
    Ok(graphql::introspection_query().to_string())
}

/// Parse a JSON introspection response body into a structured schema.
#[tauri::command]
pub fn graphql_parse_introspection(body: String) -> Result<GraphQLSchema, String> {
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("invalid JSON: {e}"))?;
    graphql::parse_introspection(&value).map_err(|e| e.to_string())
}

/// Build a deterministic GraphQL attack plan. `schema_json` is optional —
/// when provided, the alias-overload case uses an actual query-type field
/// instead of `__typename`.
#[tauri::command]
pub fn graphql_build_attack_plan(
    schema_json: Option<String>,
) -> Result<Vec<GraphQLAttackCase>, String> {
    let schema: Option<GraphQLSchema> = match schema_json {
        Some(s) if !s.trim().is_empty() => {
            Some(serde_json::from_str(&s).map_err(|e| format!("invalid schema JSON: {e}"))?)
        }
        _ => None,
    };
    Ok(graphql::build_attack_plan(schema.as_ref()))
}

// ---------------------------------------------------------------------------
// PCAP export (Feature GG)
// ---------------------------------------------------------------------------

/// Export the current history (or a filtered subset) as a libpcap file.
/// `flow_ids` is optional — when empty, every flow is exported.
#[tauri::command]
pub fn pcap_export_cmd(
    state: State<'_, AppStateSlot>,
    path: String,
    flow_ids: Option<Vec<String>>,
) -> Result<usize, String> {
    let bytes = with_state(&state, |s| {
        let mut flows: Vec<_> = s.history.list().into_iter().map(|e| e.flow).collect();
        if let Some(ids) = &flow_ids {
            let allow: std::collections::HashSet<&String> = ids.iter().collect();
            flows.retain(|f| allow.contains(&f.id.to_string()));
        }
        flows
    })?;
    let count = bytes.len();
    let out = pcap::write_pcap(&bytes).map_err(|e| e.to_string())?;
    std::fs::write(&path, out).map_err(|e| format!("write {path}: {e}"))?;
    Ok(count)
}

// ---------------------------------------------------------------------------
// Compliance reports (Feature II)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceBuildArgs {
    pub issues: Vec<Issue>,
    pub frameworks: Vec<ComplianceFramework>,
}

#[tauri::command]
pub fn compliance_build_cmd(args: ComplianceBuildArgs) -> Result<ComplianceReport, String> {
    Ok(compliance::build_report(&args.issues, &args.frameworks))
}

#[tauri::command]
pub fn compliance_render_html_cmd(report: ComplianceReport) -> Result<String, String> {
    Ok(compliance::render_html(&report))
}

#[tauri::command]
pub fn compliance_render_md_cmd(report: ComplianceReport) -> Result<String, String> {
    Ok(compliance::render_markdown(&report))
}

// ---------------------------------------------------------------------------
// Embedded Chromium browser (Feature DD)
// ---------------------------------------------------------------------------

/// Open a new Tauri webview window pre-configured to route through the
/// NyxProxy listener. The browser uses the OS's webview (WebKitGTK on
/// Linux, WebView2 on Windows, WKWebView on macOS) — all three respect
/// the per-webview proxy URL we pass them.
///
/// `target_url` is the page to open. `proxy_url` overrides the
/// `http://host:port` we route through; when omitted we use the proxy's
/// current listen address.
#[tauri::command]
pub async fn open_embedded_browser_cmd(
    app: AppHandle,
    state: State<'_, AppStateSlot>,
    target_url: String,
    proxy_url: Option<String>,
) -> Result<String, String> {
    let listen = with_state(&state, |s| s.proxy.snapshot_config().listen_addr.clone())?;
    let proxy = proxy_url.unwrap_or_else(|| format!("http://{listen}"));
    let target = url::Url::parse(&target_url).map_err(|e| format!("bad target URL: {e}"))?;
    let proxy_parsed = url::Url::parse(&proxy).map_err(|e| format!("bad proxy URL: {e}"))?;
    let label = format!("nyx-browser-{}", uuid::Uuid::new_v4().simple());
    tauri::WebviewWindowBuilder::new(
        &app,
        label.clone(),
        tauri::WebviewUrl::External(target),
    )
    .title(format!("NyxProxy Browser — proxy {proxy}"))
    .inner_size(1280.0, 800.0)
    .proxy_url(proxy_parsed)
    .build()
    .map_err(|e| format!("create webview: {e}"))?;
    Ok(label)
}

// ---------------------------------------------------------------------------
// Self-hosting wizard (Feature Y)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfHostRenderArgs {
    pub config: SelfHostConfig,
}

#[tauri::command]
pub fn selfhost_render_cmd(args: SelfHostRenderArgs) -> Result<SelfHostBundle, String> {
    Ok(selfhost::render(&args.config))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfHostWriteArgs {
    pub config: SelfHostConfig,
    pub output_dir: String,
}

#[tauri::command]
pub fn selfhost_write_cmd(args: SelfHostWriteArgs) -> Result<Vec<String>, String> {
    let bundle = selfhost::render(&args.config);
    let dir = std::path::PathBuf::from(&args.output_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;
    let mut written = Vec::new();
    let mut write = |name: &str, body: &str| -> Result<(), String> {
        let p = dir.join(name);
        std::fs::write(&p, body).map_err(|e| format!("write {name}: {e}"))?;
        written.push(p.display().to_string());
        Ok(())
    };
    write("Dockerfile", &bundle.dockerfile)?;
    write("docker-compose.yml", &bundle.compose)?;
    write(".env.example", &bundle.env_example)?;
    if let Some(caddy) = &bundle.caddyfile {
        write("Caddyfile", caddy)?;
    }
    write("README.md", &bundle.readme)?;
    Ok(written)
}

// ---------------------------------------------------------------------------
// .nyxshare encrypted evidence packs (Leapfrog #8)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareCreateArgs {
    pub password: String,
    pub note: String,
    pub flow_ids: Vec<String>,
    pub issues: Vec<Issue>,
}

#[tauri::command]
pub fn share_seal_cmd(
    state: State<'_, AppStateSlot>,
    args: ShareCreateArgs,
) -> Result<Vec<u8>, String> {
    use chrono::Utc;
    let history = with_state(&state, |s| s.history.clone())?;
    let all = history.list();
    let wanted: std::collections::HashSet<String> = args.flow_ids.into_iter().collect();
    let flows: Vec<_> = all
        .into_iter()
        .filter(|e| wanted.is_empty() || wanted.contains(&e.flow.id.to_string()))
        .map(|e| e.flow.clone())
        .collect();
    let manifest = ShareManifest {
        created_at: Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        note: args.note,
        flow_count: flows.len(),
        issue_count: args.issues.len(),
    };
    let payload = SharePayload {
        manifest,
        flows,
        issues: args.issues,
    };
    nyxshare::seal(&payload, &args.password).map_err(|e| e.to_string())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareOpenArgs {
    pub password: String,
    pub bytes: Vec<u8>,
}

#[tauri::command]
pub fn share_unseal_cmd(args: ShareOpenArgs) -> Result<SharePayload, String> {
    nyxshare::unseal(&args.bytes, &args.password).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Continuous monitoring (Feature AA)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorCreateArgs {
    pub name: String,
    pub target_url: String,
    pub scope_hosts: Vec<String>,
    pub cadence: Cadence,
}

#[tauri::command]
pub fn monitor_upsert_cmd(
    state: State<'_, AppStateSlot>,
    args: MonitorCreateArgs,
) -> Result<MonitorSchedule, String> {
    let mon = with_state(&state, |s| s.monitor.clone())?;
    let sched = MonitorSchedule::new(args.name, args.target_url, args.scope_hosts, args.cadence);
    mon.lock().upsert(sched.clone());
    with_state(&state, |s| s.persist_monitor())?;
    Ok(sched)
}

#[tauri::command]
pub fn monitor_list_cmd(state: State<'_, AppStateSlot>) -> Result<Vec<MonitorSchedule>, String> {
    let mon = with_state(&state, |s| s.monitor.clone())?;
    let v = mon.lock().list();
    Ok(v)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorIdArgs {
    pub id: String,
}

#[tauri::command]
pub fn monitor_remove_cmd(
    state: State<'_, AppStateSlot>,
    args: MonitorIdArgs,
) -> Result<(), String> {
    let id = uuid::Uuid::parse_str(&args.id).map_err(|e| format!("bad id: {e}"))?;
    let mon = with_state(&state, |s| s.monitor.clone())?;
    mon.lock().remove(id);
    with_state(&state, |s| s.persist_monitor())?;
    Ok(())
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorRunCompleteArgs {
    pub schedule_id: String,
    pub issues: Vec<Issue>,
    pub error: Option<String>,
}

#[tauri::command]
pub fn monitor_complete_run_cmd(
    state: State<'_, AppStateSlot>,
    args: MonitorRunCompleteArgs,
) -> Result<Option<MonitorRunRecord>, String> {
    use chrono::Utc;
    let id = uuid::Uuid::parse_str(&args.schedule_id).map_err(|e| format!("bad id: {e}"))?;
    let mon = with_state(&state, |s| s.monitor.clone())?;
    let now = Utc::now();
    let rec = mon
        .lock()
        .complete_run(id, now, now, &args.issues, args.error);
    with_state(&state, |s| s.persist_monitor())?;
    Ok(rec)
}

#[tauri::command]
pub fn monitor_runs_cmd(state: State<'_, AppStateSlot>) -> Result<Vec<MonitorRunRecord>, String> {
    let mon = with_state(&state, |s| s.monitor.clone())?;
    let v = mon.lock().runs.clone();
    Ok(v)
}

// ---------------------------------------------------------------------------
// Live OWASP Top-10 dashboard (Leapfrog #6)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwaspDashboardArgs {
    pub issues: Vec<Issue>,
}

#[tauri::command]
pub fn owasp_dashboard_cmd(args: OwaspDashboardArgs) -> Result<OwaspDashboard, String> {
    Ok(owasp_dashboard::build(&args.issues))
}

#[tauri::command]
pub fn write_bytes_cmd(path: String, bytes: Vec<u8>) -> Result<usize, String> {
    use std::io::Write;
    let mut f = std::fs::File::create(&path).map_err(|e| format!("create: {e}"))?;
    f.write_all(&bytes).map_err(|e| format!("write: {e}"))?;
    Ok(bytes.len())
}

#[tauri::command]
pub fn read_bytes_cmd(path: String) -> Result<Vec<u8>, String> {
    std::fs::read(&path).map_err(|e| format!("read: {e}"))
}
