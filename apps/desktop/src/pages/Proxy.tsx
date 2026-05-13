import { useEffect, useMemo, useState } from "react";

import { RequestViewer } from "@/components/RequestViewer";
import { SplitPane } from "@/components/SplitPane";
import { formatBytes, statusBucket } from "@/lib/codec";
import { useAppStore } from "@/state/store";
import { InterceptApi, listen } from "@/tauri/api";
import type {
  HistoryEntry,
  InterceptEntry,
  InterceptUpdate,
} from "@/tauri/types";

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
  const toast = useAppStore((s) => s.toast);
  const [queue, setQueue] = useState<InterceptEntry[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [editBody, setEditBody] = useState<string>("");
  const [editedBodies, setEditedBodies] = useState<Record<string, string>>({});

  const refresh = async () => {
    try {
      setQueue(await InterceptApi.list());
    } catch (err) {
      toast("error", `Intercept list failed: ${err}`);
    }
  };

  useEffect(() => {
    refresh();
    let off: (() => void) | undefined;
    listen<InterceptUpdate>("nyxproxy://intercept", (update) => {
      if (update.type === "enqueued") {
        setQueue((q) => (q.some((e) => e.id === update.id) ? q : [...q, update]));
      } else if (update.type === "resolved") {
        setQueue((q) => q.filter((e) => e.id !== update.id));
        setEditedBodies((eb) => {
          const next = { ...eb };
          delete next[update.id];
          return next;
        });
        if (selectedId === update.id) {
          setSelectedId(null);
          setEditBody("");
        }
      }
    }).then((u) => (off = u));
    return () => off?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selected = useMemo(
    () => queue.find((e) => e.id === selectedId) ?? queue[0] ?? null,
    [queue, selectedId],
  );

  // Whenever the visible selection changes, reset the textarea to the queued
  // body (decoded). If the user previously edited that entry, restore the
  // pending edit.
  useEffect(() => {
    if (!selected) {
      setEditBody("");
      return;
    }
    const prior = editedBodies[selected.id];
    if (prior !== undefined) {
      setEditBody(prior);
    } else {
      setEditBody(atobSafe(selected.body_b64));
    }
  }, [selected, editedBodies]);

  const setEditedBody = (val: string) => {
    setEditBody(val);
    if (selected) {
      setEditedBodies((eb) => ({ ...eb, [selected.id]: val }));
    }
  };

  const forward = async (id: string) => {
    try {
      const edited = editedBodies[id];
      const b64 = edited !== undefined ? btoaSafe(edited) : undefined;
      const ok = await InterceptApi.forward(id, undefined, b64);
      if (!ok) toast("warning", "Entry already resolved.");
    } catch (err) {
      toast("error", `Forward failed: ${err}`);
    }
  };

  const drop = async (id: string) => {
    try {
      await InterceptApi.drop(id);
    } catch (err) {
      toast("error", `Drop failed: ${err}`);
    }
  };

  const dropAll = async () => {
    try {
      const n = await InterceptApi.dropAll();
      toast("info", `Dropped ${n} pending request${n === 1 ? "" : "s"}.`);
    } catch (err) {
      toast("error", `Drop-all failed: ${err}`);
    }
  };

  return (
    <SplitPane
      storageKey="proxy-intercept"
      initialSize={0.35}
      first={
        <div className="panel" style={{ height: "100%", borderRadius: 0, border: "none" }}>
          <div className="toolbar" style={{ gap: 8 }}>
            <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
              <input
                type="checkbox"
                checked={!!config?.intercept_enabled}
                onChange={(e) =>
                  config && save({ ...config, intercept_enabled: e.target.checked })
                }
              />
              <span>Intercept enabled</span>
            </label>
            <button className="btn small ghost" onClick={refresh}>
              Refresh
            </button>
            <button
              className="btn small danger"
              onClick={dropAll}
              disabled={queue.length === 0}
            >
              Drop all
            </button>
          </div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            {queue.length === 0 ? (
              <div className="empty-state">
                <p>
                  No pending requests. Toggle <strong>Intercept enabled</strong> and send
                  traffic through the proxy — each request will appear here for review.
                </p>
              </div>
            ) : (
              queue.map((entry) => (
                <div
                  key={entry.id}
                  className={`nav-item ${selectedId === entry.id ? "active" : ""}`}
                  onClick={() => setSelectedId(entry.id)}
                  style={{ alignItems: "flex-start", flexDirection: "column", gap: 2 }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: 6, width: "100%" }}>
                    <span className={`pill method-${entry.captured.method}`}>
                      {entry.captured.method}
                    </span>
                    <span className="mono" style={{ flex: 1, fontSize: 12 }}>
                      {entry.captured.authority}
                    </span>
                  </div>
                  <span className="mono" style={{ fontSize: 11, color: "var(--text-dim)" }}>
                    {entry.captured.path}
                  </span>
                </div>
              ))
            )}
          </div>
        </div>
      }
      second={
        <div className="panel" style={{ height: "100%", borderRadius: 0, border: "none" }}>
          <div className="panel-header">
            {selected
              ? `${selected.captured.method} ${selected.captured.authority}${selected.captured.path}`
              : "Select a held request"}
            <div style={{ marginLeft: "auto", display: "flex", gap: 6 }}>
              {selected && (
                <>
                  <button className="btn small primary" onClick={() => forward(selected.id)}>
                    Forward
                  </button>
                  <button className="btn small danger" onClick={() => drop(selected.id)}>
                    Drop
                  </button>
                </>
              )}
            </div>
          </div>
          <div className="panel-body" style={{ padding: 12, gap: 8, overflow: "auto" }}>
            {!selected ? (
              <div className="empty-state">
                <p>Pick a request on the left to edit before forwarding.</p>
              </div>
            ) : (
              <>
                <div>
                  <div className="label">URL</div>
                  <code className="mono">{selected.captured.url}</code>
                </div>
                <div>
                  <div className="label">Headers ({selected.captured.headers.length})</div>
                  <pre className="code" style={{ maxHeight: 180, overflow: "auto" }}>
                    {selected.captured.headers
                      .map((h) => `${h.name}: ${h.value}`)
                      .join("\n")}
                  </pre>
                </div>
                <div style={{ flex: 1, display: "flex", flexDirection: "column" }}>
                  <div className="label">
                    Body (edit and click Forward to send the modified copy)
                  </div>
                  <textarea
                    className="code-input"
                    style={{ minHeight: 200, flex: 1 }}
                    value={editBody}
                    onChange={(e) => setEditedBody(e.target.value)}
                  />
                </div>
              </>
            )}
          </div>
        </div>
      }
    />
  );
}

function atobSafe(b64: string): string {
  try {
    return decodeURIComponent(
      atob(b64)
        .split("")
        .map((c) => "%" + c.charCodeAt(0).toString(16).padStart(2, "0"))
        .join(""),
    );
  } catch {
    return "";
  }
}

function btoaSafe(s: string): string {
  return btoa(
    encodeURIComponent(s).replace(/%([0-9A-F]{2})/g, (_, h) =>
      String.fromCharCode(parseInt(h, 16)),
    ),
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


