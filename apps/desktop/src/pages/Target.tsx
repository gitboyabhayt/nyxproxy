import { useMemo, useState } from "react";

import { SplitPane } from "@/components/SplitPane";
import { statusBucket } from "@/lib/codec";
import { useAppStore } from "@/state/store";

type SubTab = "site-map" | "scope" | "issues";

export function TargetPage() {
  const config = useAppStore((s) => s.proxy.config);
  const saveProxyConfig = useAppStore((s) => s.saveProxyConfig);
  const [tab, setTab] = useState<SubTab>("site-map");

  return (
    <>
      <div className="sub-tabs">
        {(
          [
            ["site-map", "Site map"],
            ["scope", "Scope"],
            ["issues", "Issues"],
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
        {tab === "site-map" && <SiteMap />}
        {tab === "scope" && (
          <ScopeEditor
            config={config}
            onSave={(c) => saveProxyConfig(c)}
          />
        )}
        {tab === "issues" && <IssuesTab />}
      </div>
    </>
  );
}

function SiteMap() {
  const history = useAppStore((s) => s.history);
  const [selectedHost, setSelectedHost] = useState<string | null>(null);

  const tree = useMemo(() => {
    const hosts = new Map<string, Array<{ method: string; path: string; status: number | null }>>();
    for (const entry of history) {
      const host = entry.flow.request.authority;
      const arr = hosts.get(host) ?? [];
      arr.push({
        method: entry.flow.request.method,
        path: entry.flow.request.path,
        status: entry.flow.response?.status ?? null,
      });
      hosts.set(host, arr);
    }
    return Array.from(hosts.entries())
      .map(([host, paths]) => ({ host, paths }))
      .sort((a, b) => a.host.localeCompare(b.host));
  }, [history]);

  const selected = tree.find((t) => t.host === selectedHost);

  return (
    <SplitPane
      storageKey="target-sitemap"
      initialSize={0.3}
      first={
        <div className="panel" style={{ height: "100%", borderRadius: 0, border: "none" }}>
          <div className="panel-header">Hosts ({tree.length})</div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            {tree.length === 0 ? (
              <div className="empty-state">
                <p>No hosts yet. Capture some traffic to build the site map.</p>
              </div>
            ) : (
              tree.map((node) => (
                <div
                  key={node.host}
                  className={`nav-item ${selectedHost === node.host ? "active" : ""}`}
                  onClick={() => setSelectedHost(node.host)}
                >
                  <span>{node.host}</span>
                  <span className="badge">{node.paths.length}</span>
                </div>
              ))
            )}
          </div>
        </div>
      }
      second={
        <div className="panel" style={{ height: "100%", borderRadius: 0, border: "none" }}>
          <div className="panel-header">{selected ? selected.host : "Select a host"}</div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            {!selected ? (
              <div className="empty-state">
                <p>Pick a host on the left to view its endpoints.</p>
              </div>
            ) : (
              <table className="data-table">
                <thead>
                  <tr>
                    <th style={{ width: 80 }}>Method</th>
                    <th>Path</th>
                    <th style={{ width: 100 }}>Status</th>
                  </tr>
                </thead>
                <tbody>
                  {selected.paths.map((p, i) => (
                    <tr key={`${p.path}-${i}`}>
                      <td>
                        <span className={`pill method-${p.method}`}>{p.method}</span>
                      </td>
                      <td>{p.path}</td>
                      <td>
                        <span className={`status-badge ${statusBucket(p.status ?? 0)}`}>
                          {p.status ?? "—"}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </div>
      }
    />
  );
}

interface ScopeEditorProps {
  config: import("@/tauri/types").ProxyConfig | null;
  onSave: (c: import("@/tauri/types").ProxyConfig) => void;
}

function ScopeEditor({ config, onSave }: ScopeEditorProps) {
  if (!config) return <div className="banner">Loading scope…</div>;
  return (
    <div className="section" style={{ overflow: "auto" }}>
      <div className="notice">
        Scope rules control which hosts NyxProxy intercepts and which it tunnels opaquely. Hosts matching any include rule
        will be intercepted; hosts matching an exclude rule are passed through untouched.
      </div>
      <ScopeList
        label="Include — only intercept these hosts (empty = all)"
        items={config.scope_include}
        onChange={(items) => onSave({ ...config, scope_include: items })}
      />
      <ScopeList
        label="Exclude — never intercept these hosts"
        items={config.scope_exclude}
        onChange={(items) => onSave({ ...config, scope_exclude: items })}
      />
    </div>
  );
}

function ScopeList({
  label,
  items,
  onChange,
}: {
  label: string;
  items: string[];
  onChange: (next: string[]) => void;
}) {
  const [input, setInput] = useState("");
  return (
    <div className="panel">
      <div className="panel-header">{label}</div>
      <div className="panel-body" style={{ padding: 10, gap: 8 }}>
        <div style={{ display: "flex", gap: 8 }}>
          <input
            placeholder="host substring, e.g. example.com"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            style={{ flex: 1 }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && input.trim()) {
                onChange([...items, input.trim()]);
                setInput("");
              }
            }}
          />
          <button
            className="btn primary"
            onClick={() => {
              if (input.trim()) {
                onChange([...items, input.trim()]);
                setInput("");
              }
            }}
          >
            Add
          </button>
        </div>
        <div>
          {items.length === 0 && <div className="notice">No rules.</div>}
          {items.map((item, i) => (
            <div
              key={`${item}-${i}`}
              style={{
                display: "flex",
                alignItems: "center",
                padding: "4px 6px",
                borderBottom: "1px solid var(--border)",
              }}
            >
              <span className="mono" style={{ flex: 1 }}>
                {item}
              </span>
              <button
                className="btn ghost small"
                onClick={() => onChange(items.filter((_, ix) => ix !== i))}
              >
                Remove
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function IssuesTab() {
  return (
    <div className="empty-state">
      <h3>Issues</h3>
      <p>
        The passive and active scanner ship in Phase 2 — once an issue is identified (XSS, SQLi, IDOR, missing headers,
        etc.) it will appear here with severity, evidence, and remediation guidance generated by the AI assistant.
      </p>
    </div>
  );
}
