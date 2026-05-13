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

export type IntruderAttack = "sniper";

export interface IntruderConfig {
  template: CapturedRequest;
  payloads: string[];
  attack: IntruderAttack;
  concurrency: number;
  insecure: boolean;
}

export interface IntruderAttempt {
  index: number;
  payload: string;
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
