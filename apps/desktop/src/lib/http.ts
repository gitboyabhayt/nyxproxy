import type { CapturedRequest, CapturedResponse, HeaderEntry } from "@/tauri/types";
import { base64ToText } from "@/lib/codec";

export function rawRequest(req: CapturedRequest): string {
  const lines: string[] = [];
  const path = req.path || "/";
  lines.push(`${req.method} ${path} ${req.http_version}`);
  for (const h of req.headers) lines.push(`${h.name}: ${h.value}`);
  lines.push("");
  if (req.body_size > 0) {
    lines.push(base64ToText(req.body_b64));
  }
  return lines.join("\r\n");
}

export function rawResponse(resp: CapturedResponse): string {
  const lines: string[] = [];
  lines.push(`${resp.http_version} ${resp.status} ${resp.reason || ""}`.trim());
  for (const h of resp.headers) lines.push(`${h.name}: ${h.value}`);
  lines.push("");
  if (resp.body_size > 0) {
    lines.push(base64ToText(resp.body_b64));
  }
  return lines.join("\r\n");
}

export function headersToRecord(headers: HeaderEntry[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const h of headers) {
    out[h.name] = h.value;
  }
  return out;
}

export function parseQuery(url: string): { base: string; params: Array<{ key: string; value: string }> } {
  const idx = url.indexOf("?");
  if (idx === -1) return { base: url, params: [] };
  const base = url.slice(0, idx);
  const qs = url.slice(idx + 1);
  const params = qs
    .split("&")
    .filter((s) => s.length > 0)
    .map((pair) => {
      const eq = pair.indexOf("=");
      const key = eq === -1 ? pair : pair.slice(0, eq);
      const value = eq === -1 ? "" : pair.slice(eq + 1);
      try {
        return { key: decodeURIComponent(key), value: decodeURIComponent(value) };
      } catch {
        return { key, value };
      }
    });
  return { base, params };
}
