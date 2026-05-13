import { useMemo, useState } from "react";

import { RequestViewer } from "@/components/RequestViewer";
import { SplitPane } from "@/components/SplitPane";
import { formatBytes, statusBucket } from "@/lib/codec";
import { useAppStore } from "@/state/store";
import type { HistoryEntry } from "@/tauri/types";

type SubTab = "intercept" | "history" | "ws" | "options";

export function ProxyPage() {
  const [tab, setTab] = useState<SubTab>("history");
  return (
    <>
      <div className="sub-tabs">
        {(
          [
            ["intercept", "Intercept"],
            ["history", "HTTP history"],
            ["ws", "WebSockets history"],
            ["options", "Options"],
          ] as const
        ).map(([id, label]) => (
          <div
            key={id}
            className={`sub-tab ${tab === id ? "active" : ""}`}
            onClick={() => setTab(id as SubTab)}
          >
            {label}
          </div>
        ))}
      </div>
      <div className="main-content" style={{ overflow: "hidden" }}>
        {tab === "intercept" && <InterceptPanel />}
        {tab === "history" && <HistoryPanel />}
        {tab === "ws" && (
          <div className="empty-state">
            <h3>WebSockets history</h3>
            <p>WebSocket capture lands in Phase 2 alongside HTTP/2. Today, websocket upgrades flow through opaquely.</p>
          </div>
        )}
        {tab === "options" && <OptionsPanel />}
      </div>
    </>
  );
}

function InterceptPanel() {
  const config = useAppStore((s) => s.proxy.config);
  const save = useAppStore((s) => s.saveProxyConfig);
  return (
    <div className="section" style={{ overflow: "auto" }}>
      <div className="panel">
        <div className="panel-header">Intercept queue</div>
        <div className="panel-body" style={{ padding: 14 }}>
          <div className="notice">
            Toggle <strong>Intercept enabled</strong> to hold every flow until you forward or drop it. Phase 1 captures
            every flow into history regardless — Intercept-on-hold lands in the very next milestone.
          </div>
          <label style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 10 }}>
            <input
              type="checkbox"
              checked={!!config?.intercept_enabled}
              onChange={(e) =>
                config && save({ ...config, intercept_enabled: e.target.checked })
              }
            />
            <span>Intercept enabled</span>
          </label>
        </div>
      </div>
    </div>
  );
}

function HistoryPanel() {
  const history = useAppStore((s) => s.history);
  const selectedId = useAppStore((s) => s.selectedFlowId);
  const select = useAppStore((s) => s.selectFlow);
  const clear = useAppStore((s) => s.clearHistory);
  const refresh = useAppStore((s) => s.refreshHistory);
  const upsert = useAppStore((s) => s.upsertRepeaterDraft);
  const [filter, setFilter] = useState("");
  const [methodFilter, setMethodFilter] = useState("");
  const [onlyErrors, setOnlyErrors] = useState(false);

  const filtered = useMemo(() => {
    const q = filter.toLowerCase();
    return history.filter((entry) => {
      if (methodFilter && entry.flow.request.method !== methodFilter) return false;
      if (onlyErrors && !entry.flow.error && (entry.flow.response?.status ?? 0) < 400) return false;
      if (!q) return true;
      return (
        entry.flow.request.url.toLowerCase().includes(q) ||
        entry.flow.request.method.toLowerCase().includes(q) ||
        entry.flow.request.authority.toLowerCase().includes(q)
      );
    });
  }, [history, filter, methodFilter, onlyErrors]);

  const selected = filtered.find((e) => e.flow.id === selectedId) ?? filtered[0];

  const sendToRepeater = (entry: HistoryEntry) => {
    const id = `${entry.flow.id}-${Date.now()}`;
    upsert({
      id,
      title: `${entry.flow.request.method} ${entry.flow.request.path}`,
      method: entry.flow.request.method,
      url: entry.flow.request.url,
      headers: entry.flow.request.headers,
      body: atobSafe(entry.flow.request.body_b64),
      follow_redirects: false,
      insecure: false,
    });
  };

  return (
    <>
      <div className="toolbar">
        <input
          placeholder="Filter URL / method / host…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className="grow"
        />
        <select value={methodFilter} onChange={(e) => setMethodFilter(e.target.value)}>
          <option value="">All methods</option>
          {["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
        <label style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--text-dim)" }}>
          <input
            type="checkbox"
            checked={onlyErrors}
            onChange={(e) => setOnlyErrors(e.target.checked)}
          />{" "}
          Errors only
        </label>
        <button className="btn" onClick={() => refresh()}>
          Refresh
        </button>
        <button className="btn danger" onClick={() => clear()}>
          Clear
        </button>
      </div>
      <SplitPane
        storageKey="proxy-history"
        direction="vertical"
        initialSize={0.4}
        first={
          <div style={{ overflow: "auto", height: "100%" }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th style={{ width: 24 }}></th>
                  <th style={{ width: 70 }}>Method</th>
                  <th style={{ width: 220 }}>Host</th>
                  <th>URL</th>
                  <th style={{ width: 80 }}>Status</th>
                  <th style={{ width: 90 }}>Length</th>
                  <th style={{ width: 80 }}>Time</th>
                  <th style={{ width: 110 }}>Actions</th>
                </tr>
              </thead>
              <tbody>
                {filtered.length === 0 && (
                  <tr>
                    <td colSpan={8}>
                      <div className="empty-state" style={{ padding: 30 }}>
                        <p>No flows match the current filter.</p>
                      </div>
                    </td>
                  </tr>
                )}
                {filtered.map((entry) => {
                  const status = entry.flow.response?.status ?? 0;
                  return (
                    <tr
                      key={entry.flow.id}
                      className={selected?.flow.id === entry.flow.id ? "selected" : ""}
                      onClick={() => select(entry.flow.id)}
                    >
                      <td>{entry.starred ? "★" : ""}</td>
                      <td>
                        <span className={`pill method-${entry.flow.request.method}`}>
                          {entry.flow.request.method}
                        </span>
                      </td>
                      <td>{entry.flow.request.authority}</td>
                      <td
                        style={{
                          maxWidth: 400,
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                        }}
                      >
                        {entry.flow.request.path}
                      </td>
                      <td>
                        <span className={`status-badge ${statusBucket(status)}`}>
                          {entry.flow.error ? "ERR" : status || "—"}
                        </span>
                      </td>
                      <td>{entry.flow.response ? formatBytes(entry.flow.response.body_size) : "—"}</td>
                      <td>{entry.flow.response?.elapsed_ms ?? "—"} ms</td>
                      <td>
                        <button
                          className="btn ghost small"
                          onClick={(e) => {
                            e.stopPropagation();
                            sendToRepeater(entry);
                          }}
                          title="Send to Repeater"
                        >
                          ↺ Repeater
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        }
        second={
          selected ? (
            <SplitPane
              storageKey="proxy-history-detail"
              initialSize={0.5}
              first={
                <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
                  <div className="panel-header">Request</div>
                  <RequestViewer side="request" request={selected.flow.request} />
                </div>
              }
              second={
                <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
                  <div className="panel-header">
                    Response{" "}
                    {selected.flow.response && (
                      <span className={`status-badge ${statusBucket(selected.flow.response.status)}`}>
                        {selected.flow.response.status}
                      </span>
                    )}
                  </div>
                  <RequestViewer side="response" response={selected.flow.response} />
                </div>
              }
            />
          ) : (
            <div className="empty-state">
              <p>Select a flow above to inspect the request and response.</p>
            </div>
          )
        }
      />
    </>
  );
}

function OptionsPanel() {
  const config = useAppStore((s) => s.proxy.config);
  const status = useAppStore((s) => s.proxy.status);
  const save = useAppStore((s) => s.saveProxyConfig);
  const ca = useAppStore((s) => s.ca);

  if (!config) return <div className="banner">Loading proxy options…</div>;

  return (
    <div className="section" style={{ overflow: "auto" }}>
      <div className="panel">
        <div className="panel-header">Listener</div>
        <div className="panel-body" style={{ padding: 14, gap: 12 }}>
          <div className="field">
            <label className="label">Bind address</label>
            <input
              value={config.listen_addr}
              onChange={(e) => save({ ...config, listen_addr: e.target.value })}
            />
            <span style={{ fontSize: 11, color: "var(--text-muted)" }}>
              Current status: {status?.running ? `listening on ${status.listen_addr}` : "stopped"}
            </span>
          </div>
        </div>
      </div>

      <div className="panel">
        <div className="panel-header">TLS interception</div>
        <div className="panel-body" style={{ padding: 14, gap: 10 }}>
          <div className="notice">
            NyxProxy generates a unique self-signed root CA on first launch and mints leaf certificates for each host on
            the fly. To intercept HTTPS without warnings, install this CA into your browser/system trust store.
          </div>
          <div className="kv">
            <div className="k">CA certificate</div>
            <div className="v mono">{ca?.cert_path}</div>
            <div className="k">Data directory</div>
            <div className="v mono">{ca?.data_dir}</div>
          </div>
          <details>
            <summary style={{ cursor: "pointer", color: "var(--text-dim)" }}>Show CA PEM</summary>
            <pre className="code" style={{ maxHeight: 240 }}>{ca?.cert_pem}</pre>
          </details>
        </div>
      </div>
    </div>
  );
}

function atobSafe(b64: string): string {
  if (!b64) return "";
  try {
    return atob(b64);
  } catch {
    return "";
  }
}
