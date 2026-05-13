import { useMemo, useState } from "react";

import { SplitPane } from "@/components/SplitPane";
import { textToBase64, statusBucket } from "@/lib/codec";
import { useAppStore } from "@/state/store";
import { AiApi, IntruderApi } from "@/tauri/api";
import type { CapturedRequest, IntruderAttack, IntruderAttempt } from "@/tauri/types";

const DEFAULT_PAYLOADS = `admin
guest
' OR 1=1 --
<script>alert(1)</script>
../../../../etc/passwd
%00
$(whoami)`;

const DEFAULT_PASSWORDS = `password
123456
admin
letmein
P@ssw0rd!
welcome
qwerty`;

const ATTACK_OPTIONS: { value: IntruderAttack; label: string; help: string }[] = [
  {
    value: "sniper",
    label: "Sniper",
    help: "1 payload set. Walk every marker independently: positions × payloads attempts.",
  },
  {
    value: "battering_ram",
    label: "Battering ram",
    help: "1 payload set. Replace every marker with the same payload per attempt.",
  },
  {
    value: "pitchfork",
    label: "Pitchfork",
    help: "N payload sets — one per marker. Zip the sets row-by-row.",
  },
  {
    value: "cluster_bomb",
    label: "Cluster bomb",
    help: "N payload sets — Cartesian product across every marker position.",
  },
];

function countMarkers(template: string): number {
  return Math.floor((template.match(/§/g) ?? []).length / 2);
}

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
    initialFlow ? buildTemplate(initialFlow) : sampleTemplate(),
  );
  const [attack, setAttack] = useState<IntruderAttack>("sniper");
  const [payloadSets, setPayloadSets] = useState<string[]>([DEFAULT_PAYLOADS, DEFAULT_PASSWORDS]);
  const [concurrency, setConcurrency] = useState(8);
  const [running, setRunning] = useState(false);
  const [attempts, setAttempts] = useState<IntruderAttempt[]>([]);
  const [aiBusy, setAiBusy] = useState(false);

  const markerCount = useMemo(() => countMarkers(template), [template]);
  const setsNeeded =
    attack === "sniper" || attack === "battering_ram" ? 1 : Math.max(markerCount, 1);

  const start = async () => {
    if (!target) {
      toast("warning", "Select a flow from Proxy history first.");
      return;
    }
    const usedSets = payloadSets
      .slice(0, setsNeeded)
      .map((raw) =>
        raw
          .split(/\r?\n/)
          .map((s) => s.trim())
          .filter((s) => s.length > 0),
      )
      .filter((set) => set.length > 0);
    if (usedSets.length === 0) {
      toast("warning", "Add at least one payload to the first set.");
      return;
    }
    if ((attack === "pitchfork" || attack === "cluster_bomb") && markerCount === 0) {
      toast("warning", "Add at least one §marker§ pair to the template first.");
      return;
    }
    setRunning(true);
    setAttempts([]);
    try {
      const parsed = parseTemplate(template, target);
      const result = await IntruderApi.run(`session-${Date.now()}`, {
        template: parsed,
        payload_sets: usedSets,
        attack,
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

  const askAiForPayloads = async (setIndex: number) => {
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
        parameter: `set ${setIndex + 1}`,
        attack_type: attack,
        count: 25,
      });
      const lines = resp.content
        .split(/\r?\n/)
        .map((s) => s.replace(/^[\d.\-\s*]+/, "").trim())
        .filter((s) => s.length > 0 && !s.toLowerCase().startsWith("here are"));
      if (lines.length > 0) {
        const next = [...payloadSets];
        next[setIndex] = lines.join("\n");
        setPayloadSets(next);
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

  const totalEstimate = useMemo(() => {
    const sets = payloadSets.slice(0, setsNeeded).map((raw) =>
      raw
        .split(/\r?\n/)
        .map((s) => s.trim())
        .filter((s) => s.length > 0),
    );
    switch (attack) {
      case "sniper":
        return (sets[0]?.length ?? 0) * Math.max(markerCount, 1);
      case "battering_ram":
        return sets[0]?.length ?? 0;
      case "pitchfork":
        return sets.length > 0 ? Math.min(...sets.map((s) => s.length)) : 0;
      case "cluster_bomb":
        return sets.length > 0 ? sets.reduce((acc, s) => acc * s.length, 1) : 0;
    }
  }, [attack, payloadSets, setsNeeded, markerCount]);

  return (
    <SplitPane
      storageKey="intruder"
      initialSize={0.5}
      first={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">
            Positions — wrap insertion points with{" "}
            <code className="code" style={{ padding: "0 4px" }}>§value§</code>
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
                  {h.flow.request.method} {h.flow.request.authority}
                  {h.flow.request.path}
                </option>
              ))}
            </select>
            <button className="btn primary" onClick={start} disabled={running}>
              {running ? "Running…" : "Start attack"}
            </button>
          </div>
          <div className="panel-body" style={{ padding: 12, overflow: "auto" }}>
            <textarea
              className="code-input"
              style={{ minHeight: 200 }}
              value={template}
              onChange={(e) => setTemplate(e.target.value)}
            />
            <div
              className="toolbar"
              style={{
                padding: 0,
                marginTop: 12,
                background: "transparent",
                border: "none",
                gap: 12,
                flexWrap: "wrap",
              }}
            >
              <div>
                <label className="label" style={{ display: "block", marginBottom: 4 }}>
                  Attack mode
                </label>
                <select value={attack} onChange={(e) => setAttack(e.target.value as IntruderAttack)}>
                  {ATTACK_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </div>
              <div>
                <label className="label" style={{ display: "block", marginBottom: 4 }}>
                  Concurrency
                </label>
                <input
                  type="number"
                  value={concurrency}
                  onChange={(e) => setConcurrency(Math.max(1, Number(e.target.value) || 1))}
                  style={{ width: 80 }}
                  min={1}
                  max={64}
                />
              </div>
              <div style={{ flex: 1, minWidth: 140 }}>
                <label className="label" style={{ display: "block", marginBottom: 4 }}>
                  Positions detected
                </label>
                <div style={{ fontFamily: "var(--font-mono)", fontSize: 13 }}>
                  {markerCount} marker pair{markerCount === 1 ? "" : "s"} —
                  about {totalEstimate.toLocaleString()} attempts
                </div>
              </div>
            </div>
            <p
              style={{
                margin: "10px 0 12px",
                fontSize: 12,
                color: "var(--text-muted)",
              }}
            >
              {ATTACK_OPTIONS.find((o) => o.value === attack)?.help}
            </p>
            {Array.from({ length: setsNeeded }).map((_, idx) => (
              <div key={idx} style={{ marginTop: idx === 0 ? 0 : 12 }}>
                <div
                  className="toolbar"
                  style={{
                    padding: 0,
                    background: "transparent",
                    border: "none",
                    justifyContent: "space-between",
                    marginBottom: 4,
                  }}
                >
                  <h3
                    style={{
                      margin: 0,
                      fontSize: 11,
                      color: "var(--text-muted)",
                    }}
                  >
                    PAYLOAD SET {idx + 1}
                  </h3>
                  <button
                    className="btn"
                    onClick={() => askAiForPayloads(idx)}
                    disabled={aiBusy}
                    style={{ padding: "2px 8px", fontSize: 12 }}
                  >
                    {aiBusy ? "Asking AI…" : "Ask AI"}
                  </button>
                </div>
                <textarea
                  className="code-input"
                  style={{ minHeight: 130 }}
                  value={payloadSets[idx] ?? ""}
                  onChange={(e) => {
                    const next = [...payloadSets];
                    while (next.length <= idx) next.push("");
                    next[idx] = e.target.value;
                    setPayloadSets(next);
                  }}
                />
              </div>
            ))}
          </div>
        </div>
      }
      second={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">Attempts ({attempts.length})</div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th style={{ width: 40 }}>#</th>
                  <th>Payload(s)</th>
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
                      <span className="mono">{a.payloads.join(" | ")}</span>
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
        return `${pair.slice(0, eq + 1)}§${pair.slice(eq + 1)}§`;
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
  const body: string[] = [];
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
        headers.push({
          name: trimmed.slice(0, idx).trim(),
          value: trimmed.slice(idx + 1).trim(),
        });
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
    "URL: https://example.com/login?user=§admin§&password=§password§",
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
