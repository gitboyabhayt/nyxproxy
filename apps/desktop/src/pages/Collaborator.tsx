import { useEffect, useMemo, useRef, useState } from "react";
import { Copy, PlayCircle, RefreshCw, Trash2 } from "lucide-react";

import { useAppStore } from "@/state/store";
import { CollaboratorApi } from "@/tauri/api";
import type { CollaboratorPing, CollaboratorSession } from "@/tauri/types";

const POLL_INTERVAL_MS = 4_000;

export function CollaboratorPage() {
  const toast = useAppStore((s) => s.toast);
  const backendUrl = useAppStore((s) => s.settings?.backend_url ?? "");
  const [session, setSession] = useState<CollaboratorSession | null>(null);
  const [pings, setPings] = useState<CollaboratorPing[]>([]);
  const [selected, setSelected] = useState<CollaboratorPing | null>(null);
  const [busy, setBusy] = useState(false);
  const [autoPoll, setAutoPoll] = useState(true);
  const timer = useRef<number | null>(null);

  const createSession = async () => {
    if (!backendUrl) {
      toast("error", "Configure backend URL in User options first.");
      return;
    }
    setBusy(true);
    try {
      const next = await CollaboratorApi.createSession(backendUrl);
      setSession(next);
      setPings([]);
      setSelected(null);
      toast("info", `Collaborator session ready — id ${next.session_id}.`);
    } catch (err) {
      toast("error", `Could not create session: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const refresh = async () => {
    if (!session) return;
    try {
      const next = await CollaboratorApi.listPings(backendUrl, session.session_id);
      setPings(next);
    } catch (err) {
      toast("error", `Poll failed: ${err}`);
    }
  };

  useEffect(() => {
    if (!session || !autoPoll) return;
    refresh();
    timer.current = window.setInterval(refresh, POLL_INTERVAL_MS);
    return () => {
      if (timer.current !== null) window.clearInterval(timer.current);
      timer.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session?.session_id, autoPoll]);

  const copyUrl = async () => {
    if (!session) return;
    try {
      await navigator.clipboard.writeText(session.polling_url);
      toast("info", "Polling URL copied to clipboard.");
    } catch (err) {
      toast("error", `Copy failed: ${err}`);
    }
  };

  const reset = () => {
    setSession(null);
    setPings([]);
    setSelected(null);
  };

  const sorted = useMemo(() => pings.slice().reverse(), [pings]);

  return (
    <>
      <div className="toolbar" style={{ gap: 8 }}>
        <button
          className="btn primary"
          onClick={createSession}
          disabled={busy}
          title="Allocate a new uniquely-named polling URL on the backend"
        >
          <PlayCircle size={14} /> New session
        </button>
        <button className="btn ghost" onClick={refresh} disabled={!session}>
          <RefreshCw size={14} /> Refresh
        </button>
        <label style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--text-dim)" }}>
          <input
            type="checkbox"
            checked={autoPoll}
            onChange={(e) => setAutoPoll(e.target.checked)}
          />
          <span>Auto-poll every {POLL_INTERVAL_MS / 1000}s</span>
        </label>
        <span style={{ flex: 1 }} />
        {session && (
          <button className="btn danger small" onClick={reset}>
            <Trash2 size={14} /> Close session
          </button>
        )}
      </div>

      <div className="main-content" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        {!session ? (
          <div className="empty-state">
            <h3>Collaborator</h3>
            <p>
              Allocate a polling URL on the NyxProxy backend. Embed it inside payloads (SSRF,
              blind XSS, callback exfiltration) — every HTTP hit will be recorded here with full
              headers and body preview. Sessions are HTTP only today; DNS + SMTP land in Phase 5.
            </p>
            <p>
              Backend in use: <code className="mono">{backendUrl || "—"}</code>
            </p>
          </div>
        ) : (
          <>
            <div className="panel">
              <div className="panel-header">Polling URL</div>
              <div className="panel-body" style={{ padding: 12, display: "flex", gap: 8, alignItems: "center" }}>
                <code className="mono grow" style={{ wordBreak: "break-all" }}>
                  {session.polling_url}
                </code>
                <button className="btn small ghost" onClick={copyUrl}>
                  <Copy size={14} /> Copy
                </button>
              </div>
            </div>

            <div className="panel grow" style={{ overflow: "hidden", display: "flex", flexDirection: "column" }}>
              <div className="panel-header">Interactions ({pings.length})</div>
              <div className="panel-body" style={{ flex: 1, overflow: "auto" }}>
                {sorted.length === 0 ? (
                  <div className="empty-state">
                    <p>No callbacks yet. Trigger a request to the polling URL to see it appear.</p>
                  </div>
                ) : (
                  <table className="data-table">
                    <thead>
                      <tr>
                        <th>Time</th>
                        <th>Method</th>
                        <th>Path</th>
                        <th>Query</th>
                        <th>Client</th>
                        <th>Size</th>
                      </tr>
                    </thead>
                    <tbody>
                      {sorted.map((p, i) => (
                        <tr
                          key={`${p.timestamp}-${i}`}
                          onClick={() => setSelected(p)}
                          className={
                            selected && p === selected ? "row-selected" : undefined
                          }
                          style={{ cursor: "pointer" }}
                        >
                          <td className="mono">{new Date(p.timestamp * 1000).toLocaleTimeString()}</td>
                          <td className={`pill method-${p.method}`}>{p.method}</td>
                          <td className="mono">{p.path}</td>
                          <td className="mono">{p.query || "—"}</td>
                          <td className="mono">{p.remote_addr ?? "—"}</td>
                          <td>{p.body_size}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            </div>

            {selected && (
              <div className="panel">
                <div className="panel-header">Ping detail</div>
                <div className="panel-body" style={{ padding: 12, display: "flex", flexDirection: "column", gap: 8 }}>
                  <div>
                    <div className="label">Headers ({Object.keys(selected.headers).length})</div>
                    <pre className="code" style={{ maxHeight: 200, overflow: "auto" }}>
                      {Object.entries(selected.headers)
                        .map(([k, v]) => `${k}: ${v}`)
                        .join("\n")}
                    </pre>
                  </div>
                  <div>
                    <div className="label">Body preview ({selected.body_size} bytes)</div>
                    <pre className="code" style={{ maxHeight: 200, overflow: "auto" }}>
                      {selected.body_preview || "(empty)"}
                    </pre>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </>
  );
}
