export type Codec =
  | "base64"
  | "base64_url"
  | "url"
  | "html"
  | "hex"
  | "ascii"
  | "gzip"
  | "deflate"
  | "zstd";

export interface HeaderEntry {
  name: string;
  value: string;
}

export interface CapturedRequest {
  method: string;
  url: string;
  scheme: string;
  authority: string;
  path: string;
  http_version: string;
  headers: HeaderEntry[];
  body_b64: string;
  body_size: number;
}

export interface CapturedResponse {
  status: number;
  http_version: string;
  reason: string;
  headers: HeaderEntry[];
  body_b64: string;
  body_size: number;
  elapsed_ms: number;
}

export interface HttpFlow {
  id: string;
  started_at: string;
  request: CapturedRequest;
  response: CapturedResponse | null;
  tags: string[];
  error: string | null;
}

export interface HistoryEntry {
  flow: HttpFlow;
  note: string | null;
  starred: boolean;
}

export interface ProxyConfig {
  listen_addr: string;
  intercept_enabled: boolean;
  scope_include: string[];
  scope_exclude: string[];
}

export interface ProxyStatus {
  running: boolean;
  listen_addr: string;
  history_size: number;
  ca_cert_path: string;
}

export interface ProxyEvent {
  kind: "started" | "stopped" | "flow" | "log" | "error";
  flow?: HttpFlow;
  message?: string;
  level?: string;
  listen_addr?: string;
}

export interface CaInfo {
  cert_pem: string;
  cert_path: string;
  data_dir: string;
}

export interface DecoderSmartResult {
  codec: Codec;
  success: boolean;
  output: string;
}

export interface SequencerReport {
  samples: number;
  mean_length: number;
  shannon_entropy_bits: number;
  character_classes: Record<string, number>;
  uniqueness_ratio: number;
}

export interface RepeaterRequest {
  method: string;
  url: string;
  headers: HeaderEntry[];
  body_b64: string;
  follow_redirects: boolean;
  insecure: boolean;
}

export type IntruderAttack = "sniper" | "battering_ram" | "pitchfork" | "cluster_bomb";

export interface IntruderConfig {
  template: CapturedRequest;
  /** One payload set per marker position. Sniper / battering-ram only use
   *  the first set; pitchfork / cluster-bomb expect one set per position. */
  payload_sets: string[][];
  attack: IntruderAttack;
  concurrency: number;
  insecure: boolean;
}

export interface IntruderAttempt {
  index: number;
  payloads: string[];
  status: number | null;
  response_length: number | null;
  elapsed_ms: number;
  error: string | null;
  snippet: string | null;
}

export interface AiChatMessage {
  role: string;
  content: string;
}

export interface AiChatArgs {
  messages: AiChatMessage[];
  provider?: string | null;
  model?: string | null;
  temperature: number;
  max_tokens: number;
}

export interface AiChatChoice {
  message: AiChatMessage;
}

export interface AiChatResponse {
  provider: string;
  model: string;
  choices: AiChatChoice[];
}

export interface AiAnalyzeResponse {
  provider: string;
  model: string;
  content: string;
}

export interface AiHttpRequestPayload {
  method: string;
  url: string;
  http_version: string;
  headers: Record<string, string>;
  body?: string | null;
}

export interface AiHttpResponsePayload {
  status: number;
  http_version: string;
  headers: Record<string, string>;
  body?: string | null;
}

export interface AiAnalyzeRequestBody {
  request: AiHttpRequestPayload;
  response?: AiHttpResponsePayload | null;
  provider?: string | null;
}

export interface AiPayloadRequestBody {
  request: AiHttpRequestPayload;
  parameter: string;
  attack_type: string;
  count: number;
  provider?: string | null;
}

export interface AiProvider {
  name: string;
  available: boolean;
  default_model: string;
  description: string;
}

export interface AiProvidersResponse {
  default: string;
  providers: AiProvider[];
}

export interface Settings {
  proxy: ProxyConfig;
  backend_url: string;
  backend_token: string | null;
  default_ai_provider: string;
  theme: string;
}

export type IssueSeverity = "info" | "low" | "medium" | "high" | "critical";
export type IssueConfidence = "tentative" | "firm" | "certain";

export interface Issue {
  id: string;
  flow_id: string;
  rule_id: string;
  name: string;
  severity: IssueSeverity;
  confidence: IssueConfidence;
  description: string;
  evidence: string | null;
  remediation: string | null;
  host: string;
  path: string;
}

export interface SpiderConfig {
  seed_url: string;
  scope_hosts: string[];
  max_depth: number;
  max_urls: number;
  concurrency: number;
  follow_robots: boolean;
  insecure: boolean;
}

export interface SpiderHit {
  url: string;
  depth: number;
  status: number | null;
  content_type: string | null;
  bytes: number | null;
  elapsed_ms: number;
  linked_count: number;
  error: string | null;
}

export interface ReportPayload {
  generated_at: string;
  flow_count: number;
  issue_count: number;
  by_severity: Record<string, number>;
  flows: HistoryEntry[];
  issues: Issue[];
}

export type InterceptKind = "request" | "response";
export type InterceptDecisionKind = "forward" | "drop";

export interface InterceptEntry {
  id: string;
  kind: InterceptKind;
  captured: CapturedRequest;
  body_b64: string;
  enqueued_at: string;
}

export type InterceptUpdate =
  | ({ type: "enqueued" } & InterceptEntry)
  | {
      type: "resolved";
      id: string;
      decision: InterceptDecisionKind;
    };
