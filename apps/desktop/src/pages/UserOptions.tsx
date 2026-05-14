import { useState } from "react";

import { DEFAULT_BACKEND_URL, probeBackend } from "@/lib/backend";
import { useAppStore } from "@/state/store";

type HealthState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "ok"; detail: string; latencyMs: number }
  | { kind: "error"; detail: string };

export function UserOptionsPage() {
  const settings = useAppStore((s) => s.settings);
  const ca = useAppStore((s) => s.ca);
  const save = useAppStore((s) => s.saveSettings);
  const providers = useAppStore((s) => s.providers);
  const reload = useAppStore((s) => s.reloadProviders);
  const toast = useAppStore((s) => s.toast);

  const [draft, setDraft] = useState(settings);
  const [health, setHealth] = useState<HealthState>({ kind: "idle" });

  if (!settings || !draft) return <div className="banner">Loading user options…</div>;

  const onSave = async () => {
    try {
      await save(draft);
      await reload();
      toast("info", "Settings saved.");
    } catch (err) {
      toast("error", `Save failed: ${err}`);
    }
  };

  const testConnection = async () => {
    setHealth({ kind: "checking" });
    const result = await probeBackend(draft.backend_url, draft.backend_token);
    if (result.ok) {
      setHealth({
        kind: "ok",
        detail: result.detail,
        latencyMs: result.latencyMs,
      });
      toast("info", `Backend reachable in ${result.latencyMs} ms.`);
    } else {
      setHealth({ kind: "error", detail: result.detail });
      toast("error", `Backend unreachable: ${result.detail}`);
    }
  };

  const resetBackendUrl = () => {
    setDraft({ ...draft, backend_url: DEFAULT_BACKEND_URL });
    setHealth({ kind: "idle" });
  };

  const downloadCa = () => {
    if (!ca) return;
    const blob = new Blob([ca.cert_pem], { type: "application/x-pem-file" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "nyxproxy-ca.pem";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="section" style={{ overflow: "auto" }}>
      <div>
        <h2>User options</h2>
        <p style={{ color: "var(--text-dim)" }}>
          Personal settings stored under <code className="code" style={{ padding: "0 4px" }}>{ca?.data_dir}</code>.
        </p>
      </div>

      <div className="panel">
        <div className="panel-header">Connection</div>
        <div className="panel-body" style={{ padding: 12 }}>
          <div className="field">
            <label className="label">Backend URL</label>
            <div style={{ display: "flex", gap: 6 }}>
              <input
                style={{ flex: 1 }}
                value={draft.backend_url}
                onChange={(e) =>
                  setDraft({ ...draft, backend_url: e.target.value })
                }
                spellCheck={false}
              />
              <button
                className="btn small"
                onClick={resetBackendUrl}
                title="Reset to hosted Render backend"
              >
                Reset
              </button>
              <button
                className="btn small"
                onClick={testConnection}
                disabled={health.kind === "checking"}
              >
                {health.kind === "checking" ? "Testing…" : "Test connection"}
              </button>
            </div>
            <span style={{ fontSize: 11, color: "var(--text-muted)" }}>
              Default: <code className="mono">{DEFAULT_BACKEND_URL}</code> — the
              hosted AI gateway. Point this at your own deployment for self-hosting.
            </span>
            {health.kind === "ok" && (
              <span
                style={{
                  fontSize: 11,
                  color: "var(--success)",
                  marginTop: 4,
                }}
              >
                Backend reachable · {health.detail} · {health.latencyMs} ms
              </span>
            )}
            {health.kind === "error" && (
              <span
                style={{
                  fontSize: 11,
                  color: "var(--danger)",
                  marginTop: 4,
                }}
              >
                {health.detail}
              </span>
            )}
          </div>
          <div className="field">
            <label className="label">Backend bearer token (optional)</label>
            <input
              value={draft.backend_token ?? ""}
              onChange={(e) => setDraft({ ...draft, backend_token: e.target.value || null })}
              placeholder="leave empty if the backend is unauthenticated"
            />
          </div>
          <div className="field">
            <label className="label">Default AI provider</label>
            <select
              value={draft.default_ai_provider}
              onChange={(e) => setDraft({ ...draft, default_ai_provider: e.target.value })}
            >
              {providers?.providers.map((p) => (
                <option key={p.name} value={p.name} disabled={!p.available}>
                  {p.name} {p.available ? "" : "(no key configured)"}
                </option>
              ))}
              {!providers && <option value={draft.default_ai_provider}>{draft.default_ai_provider}</option>}
            </select>
          </div>
          <button className="btn primary" onClick={onSave}>
            Save settings
          </button>
        </div>
      </div>

      <div className="panel">
        <div className="panel-header">Certificates</div>
        <div className="panel-body" style={{ padding: 12, gap: 8 }}>
          <div className="notice">
            Install the NyxProxy root CA in your browser/OS trust store to intercept HTTPS without warnings. Each
            machine generates its own unique CA — never share this file.
          </div>
          <div className="kv">
            <div className="k">CA path</div>
            <div className="v mono">{ca?.cert_path}</div>
            <div className="k">Data dir</div>
            <div className="v mono">{ca?.data_dir}</div>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <button className="btn" onClick={downloadCa}>
              Download CA (PEM)
            </button>
          </div>
        </div>
      </div>

      <div className="panel">
        <div className="panel-header">AI providers</div>
        <div className="panel-body" style={{ padding: 12 }}>
          {!providers ? (
            <div className="banner">No provider info loaded.</div>
          ) : (
            <table className="data-table">
              <thead>
                <tr>
                  <th>Provider</th>
                  <th>Default model</th>
                  <th>Status</th>
                  <th>Description</th>
                </tr>
              </thead>
              <tbody>
                {providers.providers.map((p) => (
                  <tr key={p.name}>
                    <td>
                      <strong>{p.name}</strong>
                      {providers.default === p.name && (
                        <span className="pill" style={{ marginLeft: 6 }}>default</span>
                      )}
                    </td>
                    <td>{p.default_model}</td>
                    <td>
                      <span
                        className={`status-badge ${p.available ? "status-2xx" : "status-4xx"}`}
                      >
                        {p.available ? "ready" : "unconfigured"}
                      </span>
                    </td>
                    <td style={{ whiteSpace: "normal", color: "var(--text-dim)" }}>{p.description}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}
