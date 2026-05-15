import { useEffect, useState } from "react";

import { DEFAULT_BACKEND_URL, probeBackend } from "@/lib/backend";
import { useAppStore } from "@/state/store";
import { SyncApi, type SyncStatus, type SyncWorkspace } from "@/tauri/api";

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

      <CloudSyncPanel
        backendUrl={draft.backend_url}
        token={draft.backend_token}
      />

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

interface CloudSyncPanelProps {
  backendUrl: string;
  token: string | null;
}

function CloudSyncPanel({ backendUrl, token }: CloudSyncPanelProps) {
  const toast = useAppStore((s) => s.toast);
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [owner, setOwner] = useState<string>(
    () => localStorage.getItem("nyx-sync-owner") || "",
  );
  const [workspaceId, setWorkspaceId] = useState<string>(
    () => localStorage.getItem("nyx-sync-workspace") || "default",
  );
  const [last, setLast] = useState<{ revision: number; updated_at: string } | null>(
    null,
  );
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const s = await SyncApi.status(backendUrl, token ?? undefined);
        if (!cancelled) setStatus(s);
      } catch {
        if (!cancelled) setStatus({ enabled: false, provider: null });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [backendUrl, token]);

  const push = async () => {
    if (!owner.trim() || !workspaceId.trim()) {
      toast("error", "Set an owner (email/uuid) and workspace ID first.");
      return;
    }
    setBusy(true);
    try {
      const workspace: SyncWorkspace = {
        id: workspaceId,
        owner,
        revision: (last?.revision ?? 0) + 1,
        payload: {
          settings_snapshot_at: new Date().toISOString(),
          // The desktop shell can hand off a fuller workspace.toJson() here.
          // We always include a timestamp so push is meaningful even on a
          // blank install.
        },
      };
      const res = await SyncApi.push(backendUrl, workspace, {
        token: token ?? undefined,
        expectedRevision: last?.revision ?? null,
      });
      setLast({ revision: res.workspace.revision, updated_at: res.updated_at });
      localStorage.setItem("nyx-sync-owner", owner);
      localStorage.setItem("nyx-sync-workspace", workspaceId);
      toast("info", `Pushed workspace rev ${res.workspace.revision}.`);
    } catch (err) {
      toast("error", `Push failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const pull = async () => {
    if (!owner.trim() || !workspaceId.trim()) {
      toast("error", "Set an owner and workspace ID first.");
      return;
    }
    setBusy(true);
    try {
      const res = await SyncApi.pull(backendUrl, owner, workspaceId, token ?? undefined);
      if (!res.workspace) {
        toast("info", "No workspace found on the cloud for this owner/id.");
        return;
      }
      setLast({
        revision: res.workspace.revision,
        updated_at: res.updated_at ?? new Date().toISOString(),
      });
      toast("info", `Pulled workspace rev ${res.workspace.revision}.`);
    } catch (err) {
      toast("error", `Pull failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="panel">
      <div className="panel-header">Cloud sync</div>
      <div className="panel-body" style={{ padding: 12, display: "flex", flexDirection: "column", gap: 10 }}>
        {status === null ? (
          <div className="muted" style={{ fontSize: 12 }}>
            Probing backend…
          </div>
        ) : !status.enabled ? (
          <div className="notice">
            Cloud sync is <strong>not configured</strong> on this backend. The
            self-host operator must set <code className="mono">SUPABASE_URL</code> and{" "}
            <code className="mono">SUPABASE_SERVICE_KEY</code> environment
            variables and create a <code className="mono">nyx_workspaces</code>{" "}
            table. See{" "}
            <a
              href="https://github.com/gitboyabhayt/nyxproxy/blob/main/docs/features/cloud-sync.md"
              target="_blank"
              rel="noreferrer"
            >
              docs/features/cloud-sync.md
            </a>
            .
          </div>
        ) : (
          <>
            <div className="muted" style={{ fontSize: 11 }}>
              Sync provider: <strong>{status.provider}</strong>. Workspaces are
              keyed by <code className="mono">(owner, workspace_id)</code> and
              version-stamped so two devices can detect each other's
              concurrent edits.
            </div>
            <div className="field">
              <label className="label">Owner (email or UUID)</label>
              <input
                value={owner}
                onChange={(e) => setOwner(e.target.value)}
                placeholder="you@example.com"
              />
            </div>
            <div className="field">
              <label className="label">Workspace ID</label>
              <input
                value={workspaceId}
                onChange={(e) => setWorkspaceId(e.target.value)}
                placeholder="default"
              />
            </div>
            <div className="row-wrap">
              <button className="btn primary" onClick={push} disabled={busy}>
                Push now
              </button>
              <button className="btn ghost" onClick={pull} disabled={busy}>
                Pull from cloud
              </button>
              {last && (
                <span className="muted" style={{ fontSize: 11 }}>
                  Last sync: rev {last.revision} ({new Date(last.updated_at).toLocaleString()})
                </span>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
