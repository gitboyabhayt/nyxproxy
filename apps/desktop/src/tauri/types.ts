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

export interface CollaboratorPing {
  timestamp: number;
  method: string;
  path: string;
  query: string;
  remote_addr: string | null;
  headers: Record<string, string>;
  body_preview: string;
  body_size: number;
}

export type ExtractionSource = "header" | "json_pointer" | "body_regex" | "cookie";

export interface Extraction {
  name: string;
  source: ExtractionSource;
  pattern: string;
}

export interface MacroStep {
  id: string;
  name: string;
  request: import("./types").RepeaterRequest;
  extractions: Extraction[];
}

export interface Macro {
  id: string;
  name: string;
  description: string;
  steps: MacroStep[];
  created_at: string;
  updated_at: string;
}

export interface MacroStepResult {
  step_id: string;
  step_name: string;
  request: import("./types").RepeaterRequest;
  response: CapturedResponse | null;
  extracted: Record<string, string>;
  duration_ms: number;
  error: string | null;
}

export interface MacroRunResult {
  macro_id: string;
  macro_name: string;
  started_at: string;
  steps: MacroStepResult[];
  final_variables: Record<string, string>;
  succeeded: boolean;
}

export interface CollaboratorSession {
  session_id: string;
  created_at: number;
  polling_url: string;
  pings: CollaboratorPing[];
}

// ---------------------------------------------------------------------------
// JWT toolkit (Feature Q)
// ---------------------------------------------------------------------------

export interface JwtDecoded {
  header: Record<string, unknown>;
  payload: Record<string, unknown>;
  signature_b64: string;
  signing_input: string;
}

export type JwtFindingKind =
  | "alg_none"
  | "weak_algorithm"
  | "missing_exp"
  | "expired_token"
  | "long_lived_token"
  | "kid_injection"
  | "jku_jwk_header"
  | "rsa_hmac_confusion";

export type JwtSeverity = "info" | "low" | "medium" | "high";

export interface JwtFinding {
  kind: JwtFindingKind;
  severity: JwtSeverity;
  detail: string;
}

export interface JwtBruteResult {
  tried: number;
  secret: string | null;
  elapsed_ms: number;
}

// ---------------------------------------------------------------------------
// Risk / OWASP enrichment (Features O + HH)
// ---------------------------------------------------------------------------

export interface IssueRisk {
  rule_id: string;
  score: number;
  owasp_code: string;
  owasp_title: string;
}

export interface OwaspBucket {
  code: string;
  title: string;
  count: number;
  max_score: number;
}

export interface RiskSummary {
  aggregate: number;
  by_owasp: OwaspBucket[];
}

// ---------------------------------------------------------------------------
// Workspaces (Feature D)
// ---------------------------------------------------------------------------

export interface Workspace {
  name: string;
  notes: string;
  scope: string[];
  history: HistoryEntry[];
  issues: Issue[];
  saved_at: string;
  app_version: string;
}

// ---------------------------------------------------------------------------
// WebSocket viewer (Feature A)
// ---------------------------------------------------------------------------

export type WsOpcode =
  | "continuation"
  | "text"
  | "binary"
  | "close"
  | "ping"
  | "pong"
  | "unknown";

export type WsDirection = "client_to_server" | "server_to_client";

export interface WsFrame {
  id: string;
  session_id: string;
  direction: WsDirection;
  opcode: WsOpcode;
  fin: boolean;
  masked: boolean;
  payload_b64: string;
  payload_size: number;
  text: string | null;
  captured_at: string;
  injected: boolean;
}

export interface WsSession {
  id: string;
  url: string;
  host: string;
  started_at: string;
  ended_at: string | null;
  close_code: number | null;
  close_reason: string | null;
  frame_count: number;
}

// ---- AI auto-attack / chained scan / fuzz mutator (PR #6) ----

export type VulnClass =
  | "sqli"
  | "xss"
  | "ssrf"
  | "lfi"
  | "rce"
  | "open_redirect"
  | "ssti"
  | "xxe"
  | "auth_bypass"
  | "idor"
  | "csrf"
  | "jwt"
  | "deserialization"
  | "graphql_injection"
  | "nosql"
  | "log4shell"
  | "prototype_pollution"
  | "race_condition";

export type AttackLocation = "query" | "body" | "header" | "cookie" | "path";

export type Severity = "info" | "low" | "medium" | "high" | "critical";

export interface AttackPayload {
  payload: string;
  rationale: string;
  exploitability: number;
}

export interface AttackVector {
  vuln: VulnClass;
  parameter: string;
  location: AttackLocation;
  severity: Severity;
  payloads: AttackPayload[];
}

export interface AutoAttackPlan {
  summary: string;
  vectors: AttackVector[];
  provider: string;
  model: string;
  fallbacks_tried: string[];
}

export interface FuzzMutation {
  payload: string;
  technique: string;
  bypasses: string[];
}

export interface FuzzMutateResponse {
  mutations: FuzzMutation[];
  provider: string;
  model: string;
  fallbacks_tried: string[];
}

export interface ChainScanStep {
  kind: "passive" | "active" | "report";
  title: string;
  issues: string[];
  payloads_used: string[];
  notes: string;
}

export interface ChainScanResponse {
  summary: string;
  risk_score: number;
  steps: ChainScanStep[];
  next_actions: string[];
  provider: string;
  model: string;
  fallbacks_tried: string[];
}

export interface HttpRequestPayload {
  method: string;
  url: string;
  http_version: string;
  headers: Record<string, string>;
  body: string | null;
}

export interface HttpResponsePayload {
  status: number;
  http_version: string;
  headers: Record<string, string>;
  body: string | null;
}
