import { useMemo, useState } from "react";

import { SplitPane } from "@/components/SplitPane";
import { textToBase64, statusBucket } from "@/lib/codec";
import { useAppStore } from "@/state/store";
import { AiApi, IntruderApi } from "@/tauri/api";
import type { CapturedRequest, IntruderAttempt } from "@/tauri/types";

const DEFAULT_PAYLOADS = `admin
guest
' OR 1=1 --
<script>alert(1)</script>
../../../../etc/passwd
%00
$(whoami)`;

export function IntruderPage() {
  const history = useAppStore((s) => s.history);
  const selectedFlowId = useAppStore((s) => s.selectedFlowId);
  const toast = useAppStore((s) => s.toast);

  const initialFlow = useMemo(() => {
    if (selectedFlowId) {
      const entry = history.find((h) => h.flow.id === selectedFlowId);
      if (entry) return entry.flow.request;
    }
    return history[0]?.flow.request ?? null;
  }, [history, selectedFlowId]);

  const [target, setTarget] = useState<CapturedRequest | null>(initialFlow);
  const [template, setTemplate] = useState<string>(() =>
    initialFlow ? buildTemplate(initialFlow) : sampleTemplate()
  );
  const [payloads, setPayloads] = useState<string>(DEFAULT_PAYLOADS);
  const [concurrency, setConcurrency] = useState(8);
  const [running, setRunning] = useState(false);
  const [attempts, setAttempts] = useState<IntruderAttempt[]>([]);
  const [aiBusy, setAiBusy] = useState(false);

  const start = async () => {
    if (!target) {
      toast("warning", "Select a flow from Proxy history first.");
      return;
    }
    const list = payloads
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (list.length === 0) {
      toast("warning", "Add at least one payload.");
      return;
    }
    setRunning(true);
    setAttempts([]);
    try {
      const parsed = parseTemplate(template, target);
      const result = await IntruderApi.run(`session-${Date.now()}`, {
        template: parsed,
        payloads: list,
        attack: "sniper",
        concurrency,
        insecure: false,
      });
      setAttempts(result);
    } catch (err) {
      toast("error", `Intruder failed: ${err}`);
    } finally {
      setRunning(false);
    }
  };

  const askAiForPayloads = async () => {
    if (!target) return;
    setAiBusy(true);
    try {
      const resp = await AiApi.generatePayloads({
        request: {
          method: target.method,
          url: target.url,
          http_version: target.http_version,
          headers: target.headers.reduce<Record<string, string>>((acc, h) => {
            acc[h.name] = h.value;
            return acc;
          }, {}),
          body: null,
        },
        parameter: "§",
        attack_type: "sniper",
        count: 20,
      });
      const lines = resp.content
        .split(/\r?\n/)
        .map((s) => s.replace(/^[\d.\-\s*]+/, "").trim())
        .filter((s) => s.length > 0 && !s.toLowerCase().startsWith("here are"));
      if (lines.length > 0) {
        setPayloads(lines.join("\n"));
        toast("info", `AI proposed ${lines.length} payloads.`);
      } else {
        toast("warning", "AI returned no payloads — keeping current list.");
      }
    } catch (err) {
      toast("error", `AI request failed: ${err}`);
    } finally {
      setAiBusy(false);
    }
  };

  return (
    <SplitPane
      storageKey="intruder"
      initialSize={0.5}
      first={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">
            Sniper template — mark insertion points with <code className="code" style={{ padding: "0 4px" }}>§</code>
          </div>
          <div className="toolbar">
            <select
              onChange={(e) => {
                const id = e.target.value;
                if (!id) return;
                const flow = history.find((h) => h.flow.id === id)?.flow.request;
                if (flow) {
                  setTarget(flow);
                  setTemplate(buildTemplate(flow));
                }
              }}
              defaultValue={selectedFlowId ?? ""}
              style={{ flex: 1 }}
            >
              <option value="">Load from history…</option>
              {history.slice(0, 50).map((h) => (
                <option key={h.flow.id} value={h.flow.id}>
                  {h.flow.request.method} {h.flow.request.authority}{h.flow.request.path}
                </option>
              ))}
            </select>
            <button className="btn primary" onClick={start} disabled={running}>
              {running ? "Running…" : "Start attack"}
            </button>
          </div>
          <div className="panel-body" style={{ padding: 12 }}>
            <textarea
              className="code-input"
              style={{ flex: 1, minHeight: 220 }}
              value={template}
              onChange={(e) => setTemplate(e.target.value)}
            />
            <div className="toolbar" style={{ padding: 0, marginTop: 12, background: "transparent", border: "none" }}>
              <label className="label" style={{ marginRight: 6 }}>Concurrency</label>
              <input
                type="number"
                value={concurrency}
                onChange={(e) => setConcurrency(Math.max(1, Number(e.target.value) || 1))}
                style={{ width: 80 }}
                min={1}
                max={64}
              />
              <button className="btn" onClick={askAiForPayloads} disabled={aiBusy}>
                {aiBusy ? "Asking AI…" : "Ask AI for payloads"}
              </button>
            </div>
            <h3 style={{ margin: "12px 0 4px", fontSize: 11, color: "var(--text-muted)" }}>PAYLOADS</h3>
            <textarea
              className="code-input"
              style={{ minHeight: 180 }}
              value={payloads}
              onChange={(e) => setPayloads(e.target.value)}
            />
          </div>
        </div>
      }
      second={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">
            Attempts ({attempts.length})
          </div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th style={{ width: 40 }}>#</th>
                  <th>Payload</th>
                  <th style={{ width: 80 }}>Status</th>
                  <th style={{ width: 100 }}>Length</th>
                  <th style={{ width: 80 }}>Time</th>
                  <th>Error</th>
                </tr>
              </thead>
              <tbody>
                {attempts.length === 0 && (
                  <tr>
                    <td colSpan={6}>
                      <div className="empty-state" style={{ padding: 30 }}>
                        <p>Start the attack to populate this table.</p>
                      </div>
                    </td>
                  </tr>
                )}
                {attempts.map((a) => (
                  <tr key={a.index}>
                    <td>{a.index}</td>
                    <td>
                      <span className="mono">{a.payload}</span>
                    </td>
                    <td>
                      <span className={`status-badge ${statusBucket(a.status ?? 0)}`}>
                        {a.status ?? "—"}
                      </span>
                    </td>
                    <td>{a.response_length ?? "—"}</td>
                    <td>{a.elapsed_ms} ms</td>
                    <td style={{ color: "var(--danger)" }}>{a.error ?? ""}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      }
    />
  );
}

function buildTemplate(req: CapturedRequest): string {
  const url = req.url;
  // Mark each query-string value with §
  let marked = url;
  const idx = url.indexOf("?");
  if (idx !== -1) {
    const head = url.slice(0, idx + 1);
    const tail = url
      .slice(idx + 1)
      .split("&")
      .map((pair) => {
        const eq = pair.indexOf("=");
        if (eq === -1) return pair;
        return `${pair.slice(0, eq + 1)}§`;
      })
      .join("&");
    marked = head + tail;
  }
  return [
    `Method: ${req.method}`,
    `URL: ${marked}`,
    `HTTP-Version: ${req.http_version}`,
    "",
    "Headers:",
    ...req.headers.map((h) => `  ${h.name}: ${h.value}`),
    "",
    "Body:",
    "",
  ].join("\n");
}

function parseTemplate(template: string, fallback: CapturedRequest): CapturedRequest {
  const lines = template.split(/\r?\n/);
  let method = fallback.method;
  let url = fallback.url;
  let version = fallback.http_version;
  const headers: Array<{ name: string; value: string }> = [];
  let mode: "header" | "body" | "kv" = "kv";
  let body: string[] = [];
  for (const line of lines) {
    if (mode === "kv") {
      if (line.startsWith("Method:")) method = line.slice(7).trim();
      else if (line.startsWith("URL:")) url = line.slice(4).trim();
      else if (line.startsWith("HTTP-Version:")) version = line.slice(13).trim();
      else if (line.startsWith("Headers:")) mode = "header";
    } else if (mode === "header") {
      if (line.startsWith("Body:")) {
        mode = "body";
        continue;
      }
      const trimmed = line.replace(/^\s+/, "");
      const idx = trimmed.indexOf(":");
      if (idx !== -1) {
        headers.push({ name: trimmed.slice(0, idx).trim(), value: trimmed.slice(idx + 1).trim() });
      }
    } else {
      body.push(line);
    }
  }

  const u = safeParseUrl(url);
  return {
    method,
    url,
    scheme: u?.protocol.replace(":", "") ?? fallback.scheme,
    authority: u?.host ?? fallback.authority,
    path: u ? `${u.pathname}${u.search}` : fallback.path,
    http_version: version,
    headers,
    body_b64: textToBase64(body.join("\n")),
    body_size: body.join("\n").length,
  };
}

function safeParseUrl(u: string): URL | null {
  try {
    return new URL(u);
  } catch {
    return null;
  }
}

function sampleTemplate(): string {
  return [
    "Method: GET",
    "URL: https://example.com/login?user=§",
    "HTTP-Version: HTTP/1.1",
    "",
    "Headers:",
    "  Host: example.com",
    "  User-Agent: NyxProxy/0.1",
    "",
    "Body:",
    "",
  ].join("\n");
}
