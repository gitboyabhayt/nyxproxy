/**
 * Typed wrapper around the Tauri command surface.
 *
 * In a browser dev build (without Tauri) the wrapper falls back to a
 * deterministic in-memory mock so that the React tree mounts and renders
 * meaningfully — this is what we use during `npm run dev` outside the Tauri
 * webview.
 */

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
  ProxyConfig,
  ProxyStatus,
  RepeaterRequest,
  CapturedResponse,
  ReportPayload,
  SequencerReport,
  Settings,
  SpiderConfig,
  SpiderHit,
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
};

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
    backend_url: "http://127.0.0.1:8765",
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
      case "settings_get":
        return settings as unknown as never;
      case "settings_set":
        settings = (args as { settings: Settings }).settings;
        return undefined as unknown as never;
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
