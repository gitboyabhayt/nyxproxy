import { useEffect, useState } from "react";
import { FolderOpen, Play, Plug, RefreshCw } from "lucide-react";

import { useAppStore } from "@/state/store";
import { invoke } from "@/tauri/api";
import type { Issue } from "@/tauri/types";

interface PluginManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string | null;
  command: string[];
  capabilities: string[];
}

interface PluginDescriptor {
  manifest: PluginManifest;
  manifest_path: string;
  working_dir: string;
  enabled: boolean;
}

export function ExtenderPage() {
  const toast = useAppStore((s) => s.toast);
  const [plugins, setPlugins] = useState<PluginDescriptor[]>([]);
  const [running, setRunning] = useState<string | null>(null);
  const [issues, setIssues] = useState<Issue[]>([]);
  const [scanning, setScanning] = useState(false);

  const refresh = async () => {
    try {
      const list = await invoke<PluginDescriptor[]>("plugins_list");
      setPlugins(list);
    } catch (err) {
      toast("error", `Plugin list failed: ${err}`);
    }
  };

  const reload = async () => {
    try {
      const list = await invoke<PluginDescriptor[]>("plugins_reload");
      setPlugins(list);
      toast("info", `Reloaded ${list.length} plugin${list.length === 1 ? "" : "s"}.`);
    } catch (err) {
      toast("error", `Plugin reload failed: ${err}`);
    }
  };

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const toggle = async (id: string, enabled: boolean) => {
    try {
      await invoke<boolean>("plugins_set_enabled", { id, enabled });
      setPlugins((ps) =>
        ps.map((p) => (p.manifest.id === id ? { ...p, enabled } : p)),
      );
    } catch (err) {
      toast("error", `Toggle failed: ${err}`);
    }
  };

  const runScan = async (id: string) => {
    setRunning(id);
    try {
      // For demo, scan the whole history through this single plugin.
      const found = await invoke<Issue[]>("plugins_scan_history");
      setIssues(found);
      toast(
        "info",
        `${id} → ${found.length} issue${found.length === 1 ? "" : "s"} from history.`,
      );
    } catch (err) {
      toast("error", `Scan failed: ${err}`);
    } finally {
      setRunning(null);
    }
  };

  const runAll = async () => {
    setScanning(true);
    try {
      const found = await invoke<Issue[]>("plugins_scan_history");
      setIssues(found);
      toast("info", `Plugins emitted ${found.length} issue${found.length === 1 ? "" : "s"}.`);
    } catch (err) {
      toast("error", `Scan failed: ${err}`);
    } finally {
      setScanning(false);
    }
  };

  return (
    <>
      <div className="toolbar" style={{ gap: 8 }}>
        <button className="btn primary" onClick={reload}>
          <RefreshCw size={14} /> Reload manifests
        </button>
        <button
          className="btn ghost"
          onClick={runAll}
          disabled={plugins.length === 0 || scanning}
        >
          <Play size={14} /> Run every enabled plugin against history
        </button>
        <span style={{ flex: 1 }} />
        <span style={{ color: "var(--text-dim)", fontSize: 12 }}>
          Plugins are read from <code className="mono">~/.nyxproxy/plugins/</code>
        </span>
      </div>

      <div className="main-content" style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        {plugins.length === 0 ? (
          <div className="empty-state">
            <h3>No plugins installed yet</h3>
            <p>
              Copy a plugin folder (manifest <code className="mono">plugin.json</code> +
              entrypoint script) into <code className="mono">~/.nyxproxy/plugins/</code> and
              click <strong>Reload manifests</strong>. A reference plugin lives under
              <code className="mono"> apps/desktop/plugins/example-wordpress/ </code> in the
              NyxProxy source tree — see{" "}
              <code className="mono">apps/desktop/plugins/README.md</code> for the
              JSON-RPC contract.
            </p>
          </div>
        ) : (
          <div className="cards">
            {plugins.map((p) => (
              <div className="card" key={p.manifest.id}>
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <Plug size={18} />
                  <strong style={{ flex: 1 }}>{p.manifest.name}</strong>
                  <span className="pill">v{p.manifest.version || "0.0.0"}</span>
                </div>
                <div className="sub" style={{ marginTop: 8 }}>
                  {p.manifest.description || "(no description)"}
                </div>
                <div style={{ marginTop: 8, display: "flex", flexWrap: "wrap", gap: 6 }}>
                  {p.manifest.capabilities.map((c) => (
                    <span key={c} className="pill" style={{ background: "var(--bg-3)" }}>
                      {c}
                    </span>
                  ))}
                </div>
                <div
                  style={{
                    marginTop: 10,
                    display: "flex",
                    gap: 8,
                    alignItems: "center",
                    color: "var(--text-dim)",
                    fontSize: 12,
                  }}
                >
                  <FolderOpen size={12} />
                  <code className="mono" style={{ flex: 1 }}>
                    {p.working_dir}
                  </code>
                </div>
                <div
                  style={{
                    marginTop: 12,
                    display: "flex",
                    gap: 8,
                    alignItems: "center",
                  }}
                >
                  <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
                    <input
                      type="checkbox"
                      checked={p.enabled}
                      onChange={(e) => toggle(p.manifest.id, e.target.checked)}
                    />
                    <span>Enabled</span>
                  </label>
                  <button
                    className="btn small primary"
                    disabled={!p.enabled || running !== null}
                    onClick={() => runScan(p.manifest.id)}
                  >
                    <Play size={12} />{" "}
                    {running === p.manifest.id ? "Running…" : "Run on history"}
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}

        {issues.length > 0 && (
          <div className="panel">
            <div className="panel-header">Plugin-emitted issues ({issues.length})</div>
            <div className="panel-body" style={{ overflow: "auto" }}>
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Severity</th>
                    <th>Rule</th>
                    <th>Name</th>
                    <th>Host</th>
                    <th>Path</th>
                  </tr>
                </thead>
                <tbody>
                  {issues.map((iss) => (
                    <tr key={iss.id}>
                      <td>
                        <span className={`pill sev-${iss.severity}`}>{iss.severity}</span>
                      </td>
                      <td className="mono">{iss.rule_id}</td>
                      <td>{iss.name}</td>
                      <td className="mono">{iss.host}</td>
                      <td className="mono">{iss.path}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </>
  );
}
