import { useState } from "react";
import { useAppStore } from "@/state/store";
import { WorkspaceApi } from "@/tauri/api";
import type { Workspace } from "@/tauri/types";

export function ProjectOptionsPage() {
  const config = useAppStore((s) => s.proxy.config);
  const save = useAppStore((s) => s.saveProxyConfig);
  const clearHistory = useAppStore((s) => s.clearHistory);
  const toast = useAppStore((s) => s.toast);
  const [pendingAddr, setPendingAddr] = useState<string | null>(null);

  if (!config) return <div className="banner">Loading project options…</div>;

  return (
    <div className="section" style={{ overflow: "auto" }}>
      <div>
        <h2>Project options</h2>
        <p style={{ color: "var(--text-dim)" }}>
          Persist your current scope, history and issues as a portable{" "}
          <code>.nyxproxy</code> workspace file (zstd-compressed JSON, magic{" "}
          <code>NYXPRJ</code>).
        </p>
      </div>
      <div className="panel">
        <div className="panel-header">Proxy listener</div>
        <div className="panel-body" style={{ padding: 12, gap: 8 }}>
          <div className="field">
            <label className="label">Bind address</label>
            <div style={{ display: "flex", gap: 8 }}>
              <input
                style={{ flex: 1 }}
                value={pendingAddr ?? config.listen_addr}
                onChange={(e) => setPendingAddr(e.target.value)}
              />
              <button
                className="btn primary"
                onClick={() => {
                  if (pendingAddr) {
                    save({ ...config, listen_addr: pendingAddr });
                    setPendingAddr(null);
                  }
                }}
              >
                Save
              </button>
            </div>
          </div>
        </div>
      </div>

      <WorkspacePanel toast={toast} scope={config.scope_include} />

      <div className="panel">
        <div className="panel-header">Project data</div>
        <div className="panel-body" style={{ padding: 12, gap: 8 }}>
          <p className="notice">Clearing history removes every captured flow from this session — it cannot be undone.</p>
          <button className="btn danger" onClick={() => clearHistory()}>
            Clear captured history
          </button>
        </div>
      </div>
    </div>
  );
}

interface WorkspacePanelProps {
  toast: (level: "info" | "warning" | "error", message: string) => void;
  scope: string[];
}

function WorkspacePanel({ toast, scope }: WorkspacePanelProps) {
  const [name, setName] = useState("Untitled workspace");
  const [notes, setNotes] = useState("");
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [loaded, setLoaded] = useState<Workspace | null>(null);
  const [busy, setBusy] = useState(false);

  async function pickSavePath(): Promise<string | null> {
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const chosen = await dialog.save({
        title: "Save NyxProxy workspace",
        defaultPath: `${name.replace(/\s+/g, "-")}.nyxproxy`,
        filters: [{ name: "NyxProxy Workspace", extensions: ["nyxproxy"] }],
      });
      return typeof chosen === "string" ? chosen : null;
    } catch {
      // Browser fallback
      const path = window.prompt(
        "Enter file path to save (.nyxproxy):",
        `${name.replace(/\s+/g, "-")}.nyxproxy`,
      );
      return path && path.trim() ? path.trim() : null;
    }
  }

  async function pickLoadPath(): Promise<string | null> {
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const chosen = await dialog.open({
        title: "Open NyxProxy workspace",
        multiple: false,
        filters: [{ name: "NyxProxy Workspace", extensions: ["nyxproxy"] }],
      });
      if (!chosen) return null;
      return typeof chosen === "string" ? chosen : null;
    } catch {
      const path = window.prompt("Enter file path to load (.nyxproxy):", "");
      return path && path.trim() ? path.trim() : null;
    }
  }

  const onSave = async () => {
    const path = await pickSavePath();
    if (!path) return;
    setBusy(true);
    try {
      const written = await WorkspaceApi.save({
        path,
        name,
        notes,
        scope,
      });
      setSavedPath(written);
      toast("info", `Workspace saved to ${written}`);
    } catch (err) {
      toast("error", `Save failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const onLoad = async () => {
    const path = await pickLoadPath();
    if (!path) return;
    setBusy(true);
    try {
      const ws = await WorkspaceApi.load(path);
      setLoaded(ws);
      setName(ws.name);
      setNotes(ws.notes);
      toast(
        "info",
        `Loaded "${ws.name}" — ${ws.history.length} flows, ${ws.issues.length} issues`,
      );
    } catch (err) {
      toast("error", `Load failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="panel">
      <div className="panel-header">Workspaces</div>
      <div className="panel-body" style={{ padding: 12, gap: 10 }}>
        <div className="field">
          <label className="label">Workspace name</label>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            spellCheck={false}
          />
        </div>
        <div className="field">
          <label className="label">Notes</label>
          <textarea
            className="code-input"
            style={{ minHeight: 80 }}
            value={notes}
            onChange={(e) => setNotes(e.target.value)}
            placeholder="Engagement context, scope notes, anything you want preserved alongside this workspace."
          />
        </div>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button className="btn primary" disabled={busy} onClick={onSave}>
            Save workspace…
          </button>
          <button className="btn" disabled={busy} onClick={onLoad}>
            Open workspace…
          </button>
        </div>
        {savedPath && (
          <div className="banner info">Saved to <code>{savedPath}</code></div>
        )}
        {loaded && (
          <div className="banner info">
            <b>Last loaded:</b> {loaded.name} — saved {loaded.saved_at} ·{" "}
            {loaded.history.length} flows · {loaded.issues.length} issues · v
            {loaded.app_version}
          </div>
        )}
      </div>
    </div>
  );
}
