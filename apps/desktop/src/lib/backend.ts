/**
 * Hosted defaults for the NyxProxy backend AI gateway.
 *
 * The Rust shell ultimately owns the live `backend_url` value — this constant
 * is used by the headless browser preview (mock bridge) and by UI surfaces
 * that explain the default to the user.
 *
 * A build-time override via `VITE_NYXPROXY_BACKEND_URL` keeps self-hosters
 * able to ship their own URL without patching source.
 */
export const DEFAULT_BACKEND_URL: string =
  (typeof import.meta !== "undefined" &&
    (import.meta as ImportMeta & { env?: Record<string, string> }).env
      ?.VITE_NYXPROXY_BACKEND_URL) ||
  "https://nyxproxy-backend.onrender.com";

export interface BackendHealth {
  ok: boolean;
  status: number | null;
  detail: string;
  latencyMs: number;
}

/**
 * Probe the backend's `/healthz` endpoint. Used by the User options page to
 * give the user one-click feedback that their backend is reachable before
 * routing AI calls through it.
 *
 * Treats any 2xx with a JSON body containing `status: "ok"` as healthy. Any
 * network error, non-2xx, or timeout is surfaced verbatim.
 */
export async function probeBackend(
  baseUrl: string,
  token?: string | null,
  timeoutMs = 8000,
): Promise<BackendHealth> {
  const trimmed = baseUrl.trim().replace(/\/+$/, "");
  if (!trimmed) {
    return {
      ok: false,
      status: null,
      detail: "backend URL is empty",
      latencyMs: 0,
    };
  }
  const url = `${trimmed}/healthz`;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  const started = performance.now();
  try {
    const headers: Record<string, string> = { Accept: "application/json" };
    if (token && token.trim()) {
      headers["Authorization"] = `Bearer ${token.trim()}`;
    }
    const res = await fetch(url, {
      method: "GET",
      headers,
      signal: controller.signal,
    });
    const latencyMs = Math.round(performance.now() - started);
    let detail = `HTTP ${res.status}`;
    let body: unknown = null;
    try {
      body = await res.json();
    } catch {
      // non-JSON body is fine, just keep the status text
    }
    if (body && typeof body === "object" && "status" in body) {
      detail = `status=${(body as { status: unknown }).status}`;
    }
    return {
      ok: res.ok,
      status: res.status,
      detail,
      latencyMs,
    };
  } catch (err) {
    const latencyMs = Math.round(performance.now() - started);
    return {
      ok: false,
      status: null,
      detail: err instanceof Error ? err.message : String(err),
      latencyMs,
    };
  } finally {
    clearTimeout(timer);
  }
}
