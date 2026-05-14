/**
 * Typed wrapper around the Tauri command surface.
 *
 * In a browser dev build (without Tauri) the wrapper falls back to a
 * deterministic in-memory mock so that the React tree mounts and renders
 * meaningfully — this is what we use during `npm run dev` outside the Tauri
 * webview.
 */

import { DEFAULT_BACKEND_URL } from "@/lib/backend";
import type {
  AiAnalyzeRequestBody,
  AiAnalyzeResponse,
  AiChatArgs,
  AiChatResponse,
  AiPayloadRequestBody,
  AiProvidersResponse,
  CaInfo,
  Codec,
  DecoderSmartResult,
  HistoryEntry,
  IntruderAttempt,
  IntruderConfig,
  InterceptEntry,
  Issue,
  IssueRisk,
  JwtBruteResult,
  JwtDecoded,
  JwtFinding,
  ProxyConfig,
  ProxyStatus,
  RepeaterRequest,
  CapturedResponse,
  ReportPayload,
  RiskSummary,
  SequencerReport,
  Settings,
  SpiderConfig,
  SpiderHit,
  Workspace,
  WsDirection,
  WsFrame,
  WsOpcode,
  WsSession,
  AutoAttackPlan,
  ChainScanResponse,
  FuzzMutateResponse,
  HttpRequestPayload,
  HttpResponsePayload,
  VulnClass,
} from "./types";

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
type ListenFn = <T>(
  event: string,
  handler: (event: { event: string; payload: T }) => void
) => Promise<() => void>;

interface TauriBridge {
  invoke: InvokeFn;
  listen: ListenFn;
  isReal: boolean;
}

let bridgePromise: Promise<TauriBridge> | null = null;

async function loadBridge(): Promise<TauriBridge> {
  if (bridgePromise) return bridgePromise;
  bridgePromise = (async () => {
    if (
      typeof window !== "undefined" &&
      // Tauri 2 exposes __TAURI_INTERNALS__
      (window as unknown as Record<string, unknown>)["__TAURI_INTERNALS__"]
    ) {
      const core = await import("@tauri-apps/api/core");
      const event = await import("@tauri-apps/api/event");
      return {
        invoke: core.invoke as InvokeFn,
        listen: event.listen as unknown as ListenFn,
        isReal: true,
      } satisfies TauriBridge;
    }
    return makeMockBridge();
  })();
  return bridgePromise;
}

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const bridge = await loadBridge();
  return bridge.invoke<T>(cmd, args);
}

export async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const bridge = await loadBridge();
  return bridge.listen<T>(event, (e) => handler(e.payload));
}

export async function isRunningInTauri(): Promise<boolean> {
  const bridge = await loadBridge();
  return bridge.isReal;
}

/* ---------- Typed command helpers ---------- */

export const ProxyApi = {
  status: () => invoke<ProxyStatus>("proxy_status"),
  start: () => invoke<string>("proxy_start"),
  stop: () => invoke<void>("proxy_stop"),
  getConfig: () => invoke<ProxyConfig>("proxy_get_config"),
  setConfig: (config: ProxyConfig) => invoke<void>("proxy_set_config", { config }),
};

export const HistoryApi = {
  list: () => invoke<HistoryEntry[]>("history_list"),
  get: (id: string) => invoke<HistoryEntry | null>("history_get", { id }),
  clear: () => invoke<void>("history_clear"),
  search: (query: string) => invoke<HistoryEntry[]>("history_search", { query }),
  setNote: (id: string, note: string | null) =>
    invoke<boolean>("history_set_note", { id, note }),
  setStarred: (id: string, starred: boolean) =>
    invoke<boolean>("history_set_starred", { id, starred }),
};

export const CaApi = {
  info: () => invoke<CaInfo>("ca_info"),
};

export const DecoderApi = {
  encode: (codec: Codec, input: string) =>
    invoke<string>("decoder_encode", { codec, input }),
  decode: (codec: Codec, input: string) =>
    invoke<string>("decoder_decode", { codec, input }),
  smart: (input: string) => invoke<DecoderSmartResult[]>("decoder_smart", { input }),
};

export const SequencerApi = {
  analyze: (samples: string[]) =>
    invoke<SequencerReport>("sequencer_analyze", { samples }),
};

export const RepeaterApi = {
  send: (request: RepeaterRequest) =>
    invoke<CapturedResponse>("repeater_send", { request }),
};

export const IntruderApi = {
  run: (sessionId: string, config: IntruderConfig) =>
    invoke<IntruderAttempt[]>("intruder_run", {
      args: { session_id: sessionId, config },
    }),
};

export const AiApi = {
  listProviders: () => invoke<AiProvidersResponse>("ai_list_providers"),
  chat: (args: AiChatArgs) => invoke<AiChatResponse>("ai_chat", { args }),
  analyzeRequest: (body: AiAnalyzeRequestBody) =>
    invoke<AiAnalyzeResponse>("ai_analyze_request", { body }),
  findVulns: (body: AiAnalyzeRequestBody) =>
    invoke<AiAnalyzeResponse>("ai_find_vulns", { body }),
  generatePayloads: (body: AiPayloadRequestBody) =>
    invoke<AiAnalyzeResponse>("ai_generate_payloads", { body }),
  autoAttack: (body: AiAutoAttackRequestBody) =>
    invoke<AutoAttackPlan>("ai_auto_attack", { body }),
  fuzzMutate: (body: AiFuzzMutateRequestBody) =>
    invoke<FuzzMutateResponse>("ai_fuzz_mutate", { body }),
  chainScan: (body: AiChainScanRequestBody) =>
    invoke<ChainScanResponse>("ai_chain_scan", { body }),
};

export interface AiAutoAttackRequestBody {
  request: HttpRequestPayload;
  response?: HttpResponsePayload | null;
  suspected?: VulnClass[];
  payloads_per_class: number;
  provider?: string | null;
}

export interface AiFuzzMutateRequestBody {
  seed: string;
  parameter?: string | null;
  attack_type: string;
  count: number;
  provider?: string | null;
}

export interface AiChainScanRequestBody {
  request: HttpRequestPayload;
  response?: HttpResponsePayload | null;
  issues_seen?: string[];
  provider?: string | null;
}

export const SettingsApi = {
  get: () => invoke<Settings>("settings_get"),
  set: (settings: Settings) => invoke<void>("settings_set", { settings }),
};

export const CollaboratorApi = {
  async createSession(backendUrl: string): Promise<import("./types").CollaboratorSession> {
    const url = `${backendUrl.replace(/\/$/, "")}/collaborator/sessions`;
    const res = await fetch(url, { method: "POST" });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return (await res.json()) as import("./types").CollaboratorSession;
  },
  async listPings(
    backendUrl: string,
    sessionId: string,
  ): Promise<import("./types").CollaboratorPing[]> {
    const url = `${backendUrl.replace(/\/$/, "")}/collaborator/sessions/${sessionId}/pings`;
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return (await res.json()) as import("./types").CollaboratorPing[];
  },
};

export const MacrosApi = {
  list: () => invoke<import("./types").Macro[]>("macros_list"),
  save: (macro: import("./types").Macro) =>
    invoke<import("./types").Macro>("macros_save", { macro_: macro }),
  delete: (id: string) => invoke<boolean>("macros_delete", { id }),
  run: (id: string, variables?: Record<string, string>) =>
    invoke<import("./types").MacroRunResult>("macros_run", {
      args: { id, variables: variables ?? {} },
    }),
};

export const InterceptApi = {
  list: () => invoke<InterceptEntry[]>("intercept_list"),
  forward: (id: string, request?: import("./types").CapturedRequest, bodyB64?: string) =>
    invoke<boolean>("intercept_forward", {
      args: { id, request: request ?? null, body_b64: bodyB64 ?? null },
    }),
  drop: (id: string) => invoke<boolean>("intercept_drop", { id }),
  dropAll: () => invoke<number>("intercept_drop_all"),
};

export const ScannerApi = {
  scanHistory: () => invoke<Issue[]>("scanner_scan_history"),
  scanFlow: (flowId: string) => invoke<Issue[]>("scanner_scan_flow", { flowId }),
};

export const SpiderApi = {
  run: (sessionId: string, config: SpiderConfig) =>
    invoke<SpiderHit[]>("spider_run", { args: { session_id: sessionId, config } }),
};

export const ReportApi = {
  build: () => invoke<ReportPayload>("report_build"),
  renderHtml: (report: ReportPayload) => invoke<string>("report_render_html", { report }),
  renderJson: (report: ReportPayload) => invoke<string>("report_render_json", { report }),
};

export const JwtApi = {
  decode: (token: string) => invoke<JwtDecoded>("jwt_decode_cmd", { token }),
  analyze: (token: string) => invoke<JwtFinding[]>("jwt_analyze_cmd", { token }),
  encodeHs256: (header: Record<string, unknown>, payload: Record<string, unknown>, secret: string) =>
    invoke<string>("jwt_encode_hs256_cmd", { args: { header, payload, secret } }),
  encodeNone: (header: Record<string, unknown>, payload: Record<string, unknown>) =>
    invoke<string>("jwt_encode_none_cmd", { args: { header, payload, secret: "" } }),
  bruteHs256: (token: string, candidates: string[]) =>
    invoke<JwtBruteResult>("jwt_brute_hs256_cmd", { args: { token, candidates } }),
};

export const RiskApi = {
  scoreIssue: (issue: Issue) => invoke<IssueRisk>("risk_score_issue_cmd", { issue }),
  summary: (issues: Issue[]) => invoke<RiskSummary>("risk_summary_cmd", { issues }),
};

export const WorkspaceApi = {
  save: (args: { path: string; name?: string; notes?: string; scope?: string[] }) =>
    invoke<string>("workspace_save_cmd", {
      args: {
        path: args.path,
        name: args.name ?? "",
        notes: args.notes ?? "",
        scope: args.scope ?? [],
      },
    }),
  load: (path: string) => invoke<Workspace>("workspace_load_cmd", { path }),
};

export interface BurpImportSummary {
  items_seen: number;
  items_imported: number;
  items_skipped: number;
  errors: string[];
  burp_version: string | null;
  export_time: string | null;
}

export const BurpImportApi = {
  /** Import a Burp Suite "Save items" XML export from a path on disk. */
  importFromXml: (path: string) =>
    invoke<BurpImportSummary>("burp_import_cmd", { path }),
};

export type OpenApiCategory = "auth-bypass" | "idor" | "rate-limit";

export interface OpenApiTestCase {
  category: OpenApiCategory;
  name: string;
  method: string;
  url: string;
  headers: [string, string][];
  body: string | null;
  repeat: number;
  notes: string;
}

export interface OpenApiPlan {
  version: string;
  server_url: string;
  cases: OpenApiTestCase[];
  diagnostics: string[];
}

export const OpenApiApi = {
  /** Read an OpenAPI / Swagger JSON document from disk and produce a plan. */
  buildPlan: (path: string, baseOverride?: string) =>
    invoke<OpenApiPlan>("openapi_build_plan_cmd", {
      path,
      baseOverride: baseOverride ?? null,
    }),
};

// ---------------------------------------------------------------------------
// GraphQL native (Feature R)
// ---------------------------------------------------------------------------

export type GraphQLAttackKind =
  | "introspection-enabled"
  | "alias-overload"
  | "batched-queries"
  | "deep-nesting"
  | "field-suggestion-leak";

export interface GraphQLAttackCase {
  kind: GraphQLAttackKind;
  name: string;
  method: string;
  body: string;
  repeat: number;
  notes: string;
}

export interface GraphQLType {
  name: string;
  kind: string;
  fields: string[];
}

export interface GraphQLSchema {
  query_type: string | null;
  mutation_type: string | null;
  subscription_type: string | null;
  types: GraphQLType[];
}

export const GraphQLApi = {
  listEndpoints: () => invoke<string[]>("graphql_list_endpoints"),
  introspectionQuery: () => invoke<string>("graphql_introspection_query"),
  parseIntrospection: (body: string) =>
    invoke<GraphQLSchema>("graphql_parse_introspection", { body }),
  buildAttackPlan: (schema?: GraphQLSchema) =>
    invoke<GraphQLAttackCase[]>("graphql_build_attack_plan", {
      schemaJson: schema ? JSON.stringify(schema) : null,
    }),
};

// ---------------------------------------------------------------------------
// PCAP export (Feature GG)
// ---------------------------------------------------------------------------

export const PcapApi = {
  /** Export the current history (or `flow_ids` subset) to a pcap file. */
  exportToFile: (path: string, flowIds?: string[]) =>
    invoke<number>("pcap_export_cmd", {
      path,
      flowIds: flowIds && flowIds.length > 0 ? flowIds : null,
    }),
};

// ---------------------------------------------------------------------------
// Compliance reports (Feature II)
// ---------------------------------------------------------------------------

export type ComplianceFramework =
  | "pci-dss"
  | "iso27001"
  | "soc2"
  | "hipaa"
  | "gdpr";

export interface ComplianceControl {
  framework: ComplianceFramework;
  control_id: string;
  control_title: string;
}

export interface ComplianceFinding {
  issue_name: string;
  severity: "critical" | "high" | "medium" | "low" | "info";
  url: string;
  controls: ComplianceControl[];
}

export interface FrameworkCoverage {
  framework: ComplianceFramework;
  control_id: string;
  control_title: string;
  finding_count: number;
}

export interface ComplianceReport {
  generated_at: string;
  frameworks: ComplianceFramework[];
  findings: ComplianceFinding[];
  coverage: FrameworkCoverage[];
}

export const ComplianceApi = {
  build: (issues: Issue[], frameworks: ComplianceFramework[]) =>
    invoke<ComplianceReport>("compliance_build_cmd", {
      args: { issues, frameworks },
    }),
  renderHtml: (report: ComplianceReport) =>
    invoke<string>("compliance_render_html_cmd", { report }),
  renderMarkdown: (report: ComplianceReport) =>
    invoke<string>("compliance_render_md_cmd", { report }),
};

// ---------------------------------------------------------------------------
// Embedded Chromium browser (Feature DD)
// ---------------------------------------------------------------------------

export const EmbeddedBrowserApi = {
  /** Open a new webview window pointing at `targetUrl`, routed through the
   *  configured proxy (defaults to the running NyxProxy listener). */
  open: (targetUrl: string, proxyUrl?: string) =>
    invoke<string>("open_embedded_browser_cmd", {
      targetUrl,
      proxyUrl: proxyUrl ?? null,
    }),
};

export const WebSocketApi = {
  listSessions: () => invoke<WsSession[]>("ws_list_sessions"),
  getSession: (id: string) => invoke<WsSession | null>("ws_get_session", { id }),
  frames: (sessionId: string) => invoke<WsFrame[]>("ws_frames", { sessionId }),
  replay: (args: {
    sessionId: string;
    direction: WsDirection;
    opcode: WsOpcode;
    payloadB64?: string;
    text?: string;
  }) =>
    invoke<void>("ws_replay", {
      args: {
        sessionId: args.sessionId,
        direction: args.direction,
        opcode: args.opcode,
        payloadB64: args.payloadB64 ?? null,
        text: args.text ?? null,
      },
    }),
  subscribe: (handler: (event: WsEvent) => void) =>
    listen<WsEvent>("nyxproxy://websocket", handler),
};

export type WsEvent =
  | { kind: "session_started"; session: WsSession }
  | { kind: "frame"; frame: WsFrame }
  | { kind: "session_ended"; session: WsSession };

/* ---------- Mock bridge for headless browser preview ---------- */

function makeMockBridge(): TauriBridge {
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  const history: HistoryEntry[] = [];
  let proxyRunning = false;
  let proxyConfig: ProxyConfig = {
    listen_addr: "127.0.0.1:8089",
    intercept_enabled: false,
    scope_include: [],
    scope_exclude: ["translate.googleapis.com"],
  };
  let settings: Settings = {
    proxy: proxyConfig,
    backend_url: DEFAULT_BACKEND_URL,
    backend_token: null,
    default_ai_provider: "groq",
    theme: "dark",
  };

  const invoke: InvokeFn = async (cmd, args) => {
    void args;
    switch (cmd) {
      case "proxy_status":
        return {
          running: proxyRunning,
          listen_addr: proxyConfig.listen_addr,
          history_size: history.length,
          ca_cert_path: "/mock/ca.pem",
        } as unknown as never;
      case "proxy_start":
        proxyRunning = true;
        return proxyConfig.listen_addr as unknown as never;
      case "proxy_stop":
        proxyRunning = false;
        return undefined as unknown as never;
      case "proxy_get_config":
        return proxyConfig as unknown as never;
      case "proxy_set_config":
        proxyConfig = (args as { config: ProxyConfig }).config;
        return undefined as unknown as never;
      case "history_list":
        return history as unknown as never;
      case "history_clear":
        history.length = 0;
        return undefined as unknown as never;
      case "history_search":
        return history as unknown as never;
      case "history_set_note":
        return true as unknown as never;
      case "history_set_starred":
        return true as unknown as never;
      case "ca_info":
        return {
          cert_pem: "-----BEGIN CERTIFICATE-----\nmock\n-----END CERTIFICATE-----\n",
          cert_path: "/mock/ca.pem",
          data_dir: "/mock",
        } as unknown as never;
      case "decoder_encode":
      case "decoder_decode": {
        const a = args as { codec: Codec; input: string };
        return a.input as unknown as never;
      }
      case "decoder_smart":
        return [] as unknown as never;
      case "sequencer_analyze":
        return {
          samples: 0,
          mean_length: 0,
          shannon_entropy_bits: 0,
          character_classes: {},
          uniqueness_ratio: 0,
        } as unknown as never;
      case "repeater_send":
        return {
          status: 200,
          http_version: "HTTP/1.1",
          reason: "OK",
          headers: [],
          body_b64: btoa("mock response"),
          body_size: 13,
          elapsed_ms: 42,
        } as unknown as never;
      case "intruder_run":
        return [] as unknown as never;
      case "intercept_list":
        return [] as unknown as never;
      case "intercept_forward":
      case "intercept_drop":
        return true as unknown as never;
      case "intercept_drop_all":
        return 0 as unknown as never;
      case "scanner_scan_history":
      case "scanner_scan_flow":
        return [] as unknown as never;
      case "plugins_list":
      case "plugins_reload":
      case "plugins_scan_flow":
      case "plugins_scan_history":
        return [] as unknown as never;
      case "plugins_set_enabled":
        return true as unknown as never;
      case "macros_list":
        return [] as unknown as never;
      case "macros_save":
        return (cmd === "macros_save" ? args?.macro_ : null) as unknown as never;
      case "macros_delete":
        return true as unknown as never;
      case "macros_run":
        return {
          macro_id: ((args?.args as Record<string, unknown>)?.id as string) ?? "",
          macro_name: "",
          started_at: new Date().toISOString(),
          steps: [],
          final_variables: {},
          succeeded: true,
        } as unknown as never;
      case "spider_run":
        return [] as unknown as never;
      case "report_build":
        return {
          generated_at: new Date().toISOString(),
          flow_count: 0,
          issue_count: 0,
          by_severity: {},
          flows: [],
          issues: [],
        } as unknown as never;
      case "report_render_html":
        return "<!doctype html><html><body>Mock report</body></html>" as unknown as never;
      case "report_render_json":
        return "{}" as unknown as never;
      case "ai_list_providers":
        return {
          default: "groq",
          providers: [
            { name: "groq", available: false, default_model: "llama-3.3-70b-versatile", description: "Groq (mock)" },
          ],
        } as unknown as never;
      case "ai_chat":
      case "ai_analyze_request":
      case "ai_find_vulns":
      case "ai_generate_payloads":
        return {
          provider: "mock",
          model: "mock",
          content: "Running outside the Tauri shell — connect via npm run tauri:dev for real AI calls.",
          choices: [{ message: { role: "assistant", content: "mock response" } }],
        } as unknown as never;
      case "ai_auto_attack":
        return {
          summary: "Mock plan — connect via npm run tauri:dev for real AI calls.",
          vectors: [],
          provider: "mock",
          model: "mock",
          fallbacks_tried: [],
        } as unknown as never;
      case "ai_fuzz_mutate":
        return {
          mutations: [],
          provider: "mock",
          model: "mock",
          fallbacks_tried: [],
        } as unknown as never;
      case "ai_chain_scan":
        return {
          summary: "Mock chain scan",
          risk_score: 0,
          steps: [],
          next_actions: [],
          provider: "mock",
          model: "mock",
          fallbacks_tried: [],
        } as unknown as never;
      case "settings_get":
        return settings as unknown as never;
      case "settings_set":
        settings = (args as { settings: Settings }).settings;
        return undefined as unknown as never;
      case "jwt_decode_cmd":
        return {
          header: { alg: "HS256", typ: "JWT" },
          payload: { sub: "mock", iat: 0 },
          signature_b64: "",
          signing_input: "mock.mock",
        } as unknown as never;
      case "jwt_analyze_cmd":
        return [] as unknown as never;
      case "jwt_encode_hs256_cmd":
      case "jwt_encode_none_cmd":
        return "mock.token.signature" as unknown as never;
      case "jwt_brute_hs256_cmd":
        return { tried: 0, secret: null, elapsed_ms: 0 } as unknown as never;
      case "risk_score_issue_cmd":
        return {
          rule_id: "mock",
          score: 50,
          owasp_code: "OTH",
          owasp_title: "Other",
        } as unknown as never;
      case "risk_summary_cmd":
        return { aggregate: 0, by_owasp: [] } as unknown as never;
      case "workspace_save_cmd":
        return "/mock/workspace.nyxproxy" as unknown as never;
      case "workspace_load_cmd":
        return {
          name: "mock",
          notes: "",
          scope: [],
          history: [],
          issues: [],
          saved_at: new Date().toISOString(),
          app_version: "0.0.0",
        } as unknown as never;
      case "burp_import_cmd":
        return {
          items_seen: 0,
          items_imported: 0,
          items_skipped: 0,
          errors: [],
          burp_version: null,
          export_time: null,
        } as unknown as never;
      case "openapi_build_plan_cmd":
        return {
          version: "mock",
          server_url: "https://mock.example.com",
          cases: [],
          diagnostics: ["mock: openapi plan not available in browser preview"],
        } as unknown as never;
      case "graphql_list_endpoints":
        return [] as unknown as never;
      case "graphql_introspection_query":
        return "query IntrospectionQuery { __schema { queryType { name } } }" as unknown as never;
      case "graphql_parse_introspection":
        return {
          query_type: null,
          mutation_type: null,
          subscription_type: null,
          types: [],
        } as unknown as never;
      case "graphql_build_attack_plan":
        return [] as unknown as never;
      case "pcap_export_cmd":
        return 0 as unknown as never;
      case "compliance_build_cmd":
        return {
          generated_at: new Date().toISOString(),
          frameworks: [],
          findings: [],
          coverage: [],
        } as unknown as never;
      case "compliance_render_html_cmd":
        return "<html><body>mock report</body></html>" as unknown as never;
      case "compliance_render_md_cmd":
        return "# mock report" as unknown as never;
      case "open_embedded_browser_cmd":
        return "mock-window" as unknown as never;
      default:
        throw new Error(`unsupported mock invoke: ${cmd}`);
    }
  };

  const listen: ListenFn = async (event, handler) => {
    const set = listeners.get(event) ?? new Set();
    set.add(handler as never);
    listeners.set(event, set);
    return () => {
      set.delete(handler as never);
    };
  };

  return { invoke, listen, isReal: false };
}
