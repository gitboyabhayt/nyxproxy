import { useState } from "react";
import { useAppStore } from "@/state/store";
import {
  BurpImportApi,
  EmbeddedBrowserApi,
  invoke,
  NyxShareApi,
  PcapApi,
  SelfHostApi,
  WorkspaceApi,
  type BurpImportSummary,
  type SelfHostBundle,
  type SelfHostConfig,
  type SharePayload,
} from "@/tauri/api";
import type { Issue, Workspace } from "@/tauri/types";

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

      <BurpImportPanel toast={toast} />

      <PcapExportPanel toast={toast} />

      <EmbeddedBrowserPanel
        toast={toast}
        listenAddr={config.listen_addr}
      />

      <SelfHostPanel toast={toast} />

      <NyxSharePanel toast={toast} />

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

interface BurpImportPanelProps {
  toast: (level: "info" | "warning" | "error", message: string) => void;
}

function BurpImportPanel({ toast }: BurpImportPanelProps) {
  const [busy, setBusy] = useState(false);
  const [lastSummary, setLastSummary] = useState<BurpImportSummary | null>(null);

  async function pickXmlPath(): Promise<string | null> {
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const chosen = await dialog.open({
        title: "Import Burp Suite items XML",
        multiple: false,
        filters: [
          { name: "Burp items XML", extensions: ["xml"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (!chosen) return null;
      return typeof chosen === "string" ? chosen : null;
    } catch {
      const path = window.prompt(
        "Enter path to a Burp Suite XML items export:",
        "",
      );
      return path && path.trim() ? path.trim() : null;
    }
  }

  const onImport = async () => {
    const path = await pickXmlPath();
    if (!path) return;
    setBusy(true);
    try {
      const summary = await BurpImportApi.importFromXml(path);
      setLastSummary(summary);
      if (summary.items_imported > 0) {
        toast(
          "info",
          `Imported ${summary.items_imported} flows from Burp${
            summary.items_skipped ? ` (${summary.items_skipped} skipped)` : ""
          }`,
        );
      } else {
        toast(
          "warning",
          `No items imported from Burp XML (saw ${summary.items_seen}, skipped ${summary.items_skipped})`,
        );
      }
    } catch (err) {
      toast("error", `Import failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="panel">
      <div className="panel-header">Import from Burp Suite</div>
      <div className="panel-body" style={{ padding: 12, gap: 8 }}>
        <p style={{ color: "var(--text-dim)", margin: 0 }}>
          In Burp Suite: <b>Proxy</b> → <b>HTTP history</b> → select items →
          right-click → <b>Save items…</b> (XML format). Drop the resulting{" "}
          <code>.xml</code> file here and every request/response pair is
          appended to NyxProxy history with the <code>import:burp</code> tag.
        </p>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn primary" disabled={busy} onClick={onImport}>
            Choose Burp XML…
          </button>
        </div>
        {lastSummary && (
          <div className="banner info">
            Burp <code>{lastSummary.burp_version ?? "?"}</code>: imported{" "}
            <b>{lastSummary.items_imported}</b> of{" "}
            <b>{lastSummary.items_seen}</b> items
            {lastSummary.items_skipped ? ` (${lastSummary.items_skipped} skipped)` : ""}
            {lastSummary.errors.length
              ? ` — first error: ${lastSummary.errors[0]}`
              : ""}
          </div>
        )}
      </div>
    </div>
  );
}

interface PcapExportPanelProps {
  toast: ReturnType<typeof useAppStore.getState>["toast"];
}

function PcapExportPanel({ toast }: PcapExportPanelProps) {
  const [busy, setBusy] = useState(false);

  async function pickAndExport(): Promise<void> {
    let path: string | null = null;
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const chosen = await dialog.save({
        title: "Export history as pcap",
        defaultPath: "nyxproxy.pcap",
        filters: [{ name: "PCAP", extensions: ["pcap"] }],
      });
      if (typeof chosen === "string") path = chosen;
    } catch {
      path = window.prompt(
        "Enter path for the pcap file:",
        "nyxproxy.pcap",
      );
    }
    if (!path) return;
    setBusy(true);
    try {
      const n = await PcapApi.exportToFile(path);
      toast("info", `pcap exported — ${n} flows written to ${path}`);
    } catch (err) {
      toast("error", `pcap export failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel">
      <div className="panel-header">Export as pcap (Feature GG)</div>
      <div className="panel-body" style={{ padding: 12, gap: 8 }}>
        <p style={{ color: "var(--text-dim)", margin: 0 }}>
          Save the current history as a libpcap file. Open it in Wireshark to
          inspect the synthesised HTTP frames packet-by-packet.
        </p>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn" disabled={busy} onClick={() => void pickAndExport()}>
            Export pcap…
          </button>
        </div>
      </div>
    </div>
  );
}

interface EmbeddedBrowserPanelProps {
  toast: ReturnType<typeof useAppStore.getState>["toast"];
  listenAddr: string;
}

function EmbeddedBrowserPanel({ toast, listenAddr }: EmbeddedBrowserPanelProps) {
  const [url, setUrl] = useState("https://example.com");
  const [busy, setBusy] = useState(false);

  async function launch(): Promise<void> {
    setBusy(true);
    try {
      await EmbeddedBrowserApi.open(url);
      toast("info", `Browser opened — routed through http://${listenAddr}`);
    } catch (err) {
      toast("error", `Browser launch failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel">
      <div className="panel-header">Embedded browser (Feature DD)</div>
      <div className="panel-body" style={{ padding: 12, gap: 8 }}>
        <p style={{ color: "var(--text-dim)", margin: 0 }}>
          Open a webview pre-configured to use the NyxProxy listener at{" "}
          <code>http://{listenAddr}</code>. No manual proxy setup. The CA still
          needs to be trusted by your OS so HTTPS interception works — install
          it from the "User options → CA" panel.
        </p>
        <div className="field">
          <label className="label">Target URL</label>
          <input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://example.com"
          />
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button
            className="btn primary"
            disabled={busy || url.length === 0}
            onClick={() => void launch()}
          >
            Open browser
          </button>
        </div>
      </div>
    </div>
  );
}

interface SelfHostPanelProps {
  toast: (level: "info" | "warning" | "error", message: string) => void;
}

function SelfHostPanel({ toast }: SelfHostPanelProps) {
  const [cfg, setCfg] = useState<SelfHostConfig>({
    port: 8080,
    enableCaddy: false,
    caddyHost: null,
    enableCloudflareTunnel: false,
    persistentDataVolume: true,
  });
  const [bundle, setBundle] = useState<SelfHostBundle | null>(null);
  const [busy, setBusy] = useState(false);

  async function preview(): Promise<void> {
    setBusy(true);
    try {
      const b = await SelfHostApi.render(cfg);
      setBundle(b);
      toast("info", "Self-host bundle generated");
    } catch (err) {
      toast("error", `Render failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  async function write(): Promise<void> {
    let dir: string | null = null;
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const chosen = await dialog.open({
        title: "Choose output directory",
        directory: true,
        multiple: false,
      });
      if (typeof chosen === "string") dir = chosen;
    } catch {
      dir = window.prompt("Output directory:", "./nyxproxy-selfhost");
    }
    if (!dir) return;
    setBusy(true);
    try {
      const written = await SelfHostApi.write(cfg, dir);
      toast("info", `Wrote ${written.length} files to ${dir}`);
    } catch (err) {
      toast("error", `Write failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel">
      <div className="panel-header">Self-hosting wizard</div>
      <div className="panel-body" style={{ padding: 12, gap: 8 }}>
        <p className="notice">
          Generate a ready-to-deploy Docker bundle for the NyxProxy backend.
        </p>
        <div className="field">
          <label className="label">Backend port</label>
          <input
            type="number"
            value={cfg.port}
            onChange={(e) =>
              setCfg({ ...cfg, port: parseInt(e.target.value, 10) || 8080 })
            }
          />
        </div>
        <label>
          <input
            type="checkbox"
            checked={cfg.enableCaddy}
            onChange={(e) =>
              setCfg({ ...cfg, enableCaddy: e.target.checked })
            }
          />{" "}
          Add Caddy reverse proxy with auto-TLS
        </label>
        {cfg.enableCaddy && (
          <div className="field">
            <label className="label">Public host (for Caddy TLS)</label>
            <input
              value={cfg.caddyHost ?? ""}
              onChange={(e) =>
                setCfg({ ...cfg, caddyHost: e.target.value || null })
              }
              placeholder="nyxproxy.example.com"
            />
          </div>
        )}
        <label>
          <input
            type="checkbox"
            checked={cfg.enableCloudflareTunnel}
            onChange={(e) =>
              setCfg({ ...cfg, enableCloudflareTunnel: e.target.checked })
            }
          />{" "}
          Add Cloudflare Tunnel sidecar
        </label>
        <label>
          <input
            type="checkbox"
            checked={cfg.persistentDataVolume}
            onChange={(e) =>
              setCfg({ ...cfg, persistentDataVolume: e.target.checked })
            }
          />{" "}
          Persistent data volume
        </label>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn" onClick={preview} disabled={busy}>
            Preview bundle
          </button>
          <button className="btn primary" onClick={write} disabled={busy}>
            Write files…
          </button>
        </div>
        {bundle && (
          <details>
            <summary>Generated docker-compose.yml</summary>
            <pre style={{ maxHeight: 240, overflow: "auto" }}>
              {bundle.compose}
            </pre>
          </details>
        )}
      </div>
    </div>
  );
}

interface NyxSharePanelProps {
  toast: (level: "info" | "warning" | "error", message: string) => void;
}

function NyxSharePanel({ toast }: NyxSharePanelProps) {
  const [password, setPassword] = useState("");
  const [note, setNote] = useState("");
  const [unsealed, setUnsealed] = useState<SharePayload | null>(null);
  const [busy, setBusy] = useState(false);

  async function pickSavePath(): Promise<string | null> {
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const chosen = await dialog.save({
        title: "Save .nyxshare evidence pack",
        defaultPath: "evidence.nyxshare",
        filters: [{ name: "NyxShare", extensions: ["nyxshare"] }],
      });
      return typeof chosen === "string" ? chosen : null;
    } catch {
      return window.prompt(
        "Enter file path to save (.nyxshare):",
        "evidence.nyxshare",
      );
    }
  }

  async function pickOpenPath(): Promise<string | null> {
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const chosen = await dialog.open({
        title: "Open .nyxshare evidence pack",
        multiple: false,
        filters: [{ name: "NyxShare", extensions: ["nyxshare"] }],
      });
      return typeof chosen === "string" ? chosen : null;
    } catch {
      return window.prompt("Enter file path to open (.nyxshare):", "");
    }
  }

  async function seal(): Promise<void> {
    if (!password) {
      toast("error", "Password is required");
      return;
    }
    const path = await pickSavePath();
    if (!path) return;
    setBusy(true);
    try {
      const issues: Issue[] = [];
      const bytes = await NyxShareApi.seal({
        password,
        note,
        flowIds: [],
        issues,
      });
      const written = await invoke<number>("write_bytes_cmd", { path, bytes });
      toast("info", `Sealed ${written} bytes to ${path}`);
    } catch (err) {
      toast("error", `Seal failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  async function unseal(): Promise<void> {
    if (!password) {
      toast("error", "Password is required");
      return;
    }
    const path = await pickOpenPath();
    if (!path) return;
    setBusy(true);
    try {
      const bytes = await invoke<number[]>("read_bytes_cmd", { path });
      const payload = await NyxShareApi.unseal({
        password,
        bytes,
      });
      setUnsealed(payload);
      toast(
        "info",
        `Unsealed ${payload.flows.length} flows, ${payload.issues.length} issues`,
      );
    } catch (err) {
      toast("error", `Unseal failed: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel">
      <div className="panel-header">Encrypted evidence packs (.nyxshare)</div>
      <div className="panel-body" style={{ padding: 12, gap: 8 }}>
        <p className="notice">
          Bundle the current capture into an end-to-end-encrypted file
          (ChaCha20-Poly1305 + Argon2id) that another tester can replay.
        </p>
        <div className="field">
          <label className="label">Password</label>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </div>
        <div className="field">
          <label className="label">Note (visible in manifest)</label>
          <input value={note} onChange={(e) => setNote(e.target.value)} />
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn primary" onClick={seal} disabled={busy}>
            Seal &amp; export
          </button>
          <button className="btn" onClick={unseal} disabled={busy}>
            Import &amp; unseal…
          </button>
        </div>
        {unsealed && (
          <pre style={{ maxHeight: 200, overflow: "auto" }}>
            {JSON.stringify(unsealed.manifest, null, 2)}
          </pre>
        )}
      </div>
    </div>
  );
}
