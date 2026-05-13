import { useMemo, useState } from "react";

import { SplitPane } from "@/components/SplitPane";
import { statusBucket } from "@/lib/codec";
import { useAppStore } from "@/state/store";
import { ScannerApi, SpiderApi } from "@/tauri/api";

type SubTab = "site-map" | "scope" | "issues" | "spider";

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
            ["spider", "Spider"],
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
        {tab === "spider" && <SpiderTab />}
        {tab === "issues" && <IssuesTab />}
      </div>
    </>
  );
}

function SpiderTab() {
  const toast = useAppStore((s) => s.toast);
  const config = useAppStore((s) => s.proxy.config);
  const [seedUrl, setSeedUrl] = useState("");
  const [scopeText, setScopeText] = useState(() =>
    (config?.scope_include ?? []).join("\n"),
  );
  const [maxDepth, setMaxDepth] = useState(3);
  const [maxUrls, setMaxUrls] = useState(100);
  const [concurrency, setConcurrency] = useState(4);
  const [followRobots, setFollowRobots] = useState(true);
  const [insecure, setInsecure] = useState(false);
  const [running, setRunning] = useState(false);
  const [hits, setHits] = useState<import("@/tauri/types").SpiderHit[]>([]);

  const run = async () => {
    if (!seedUrl) {
      toast("warning", "Enter a seed URL first.");
      return;
    }
    const scopeHosts = scopeText
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    setRunning(true);
    setHits([]);
    try {
      const result = await SpiderApi.run(`spider-${Date.now()}`, {
        seed_url: seedUrl,
        scope_hosts: scopeHosts,
        max_depth: maxDepth,
        max_urls: maxUrls,
        concurrency,
        follow_robots: followRobots,
        insecure,
      });
      setHits(result);
      toast(
        "info",
        `Spider finished — ${result.length} URLs visited.`,
      );
    } catch (err) {
      toast("error", `Spider failed: ${err}`);
    } finally {
      setRunning(false);
    }
  };

  return (
    <SplitPane
      storageKey="target-spider"
      initialSize={0.45}
      first={
        <div className="panel" style={{ height: "100%", borderRadius: 0, border: "none" }}>
          <div className="panel-header">Crawl configuration</div>
          <div className="panel-body" style={{ padding: 12, gap: 8 }}>
            <label className="label">Seed URL</label>
            <input
              placeholder="https://example.com/"
              value={seedUrl}
              onChange={(e) => setSeedUrl(e.target.value)}
            />
            <label className="label" style={{ marginTop: 8 }}>
              Scope hosts (one per line — empty = unrestricted)
            </label>
            <textarea
              className="code-input"
              style={{ minHeight: 90 }}
              value={scopeText}
              onChange={(e) => setScopeText(e.target.value)}
            />
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 8, marginTop: 8 }}>
              <div>
                <label className="label">Max depth</label>
                <input
                  type="number"
                  min={1}
                  max={10}
                  value={maxDepth}
                  onChange={(e) => setMaxDepth(Math.max(1, Number(e.target.value) || 1))}
                />
              </div>
              <div>
                <label className="label">Max URLs</label>
                <input
                  type="number"
                  min={1}
                  max={2000}
                  value={maxUrls}
                  onChange={(e) => setMaxUrls(Math.max(1, Number(e.target.value) || 1))}
                />
              </div>
              <div>
                <label className="label">Concurrency</label>
                <input
                  type="number"
                  min={1}
                  max={32}
                  value={concurrency}
                  onChange={(e) => setConcurrency(Math.max(1, Number(e.target.value) || 1))}
                />
              </div>
            </div>
            <label style={{ display: "flex", gap: 6, alignItems: "center", marginTop: 8 }}>
              <input
                type="checkbox"
                checked={followRobots}
                onChange={(e) => setFollowRobots(e.target.checked)}
              />
              Respect robots.txt
            </label>
            <label style={{ display: "flex", gap: 6, alignItems: "center" }}>
              <input
                type="checkbox"
                checked={insecure}
                onChange={(e) => setInsecure(e.target.checked)}
              />
              Skip TLS verification
            </label>
            <button
              className="btn primary"
              style={{ marginTop: 12 }}
              onClick={run}
              disabled={running}
            >
              {running ? "Crawling…" : "Start spider"}
            </button>
          </div>
        </div>
      }
      second={
        <div className="panel" style={{ height: "100%", borderRadius: 0, border: "none" }}>
          <div className="panel-header">Visited URLs ({hits.length})</div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th style={{ width: 40 }}>D</th>
                  <th>URL</th>
                  <th style={{ width: 70 }}>Status</th>
                  <th style={{ width: 80 }}>Bytes</th>
                  <th style={{ width: 70 }}>Links</th>
                  <th style={{ width: 80 }}>Time</th>
                  <th>Error</th>
                </tr>
              </thead>
              <tbody>
                {hits.length === 0 && (
                  <tr>
                    <td colSpan={7}>
                      <div className="empty-state" style={{ padding: 24 }}>
                        <p>Enter a seed URL and start the spider to populate this table.</p>
                      </div>
                    </td>
                  </tr>
                )}
                {hits.map((hit, i) => (
                  <tr key={`${hit.url}-${i}`}>
                    <td>{hit.depth}</td>
                    <td className="mono">{hit.url}</td>
                    <td>
                      <span className={`status-badge ${statusBucket(hit.status ?? 0)}`}>
                        {hit.status ?? "—"}
                      </span>
                    </td>
                    <td>{hit.bytes ?? "—"}</td>
                    <td>{hit.linked_count}</td>
                    <td>{hit.elapsed_ms} ms</td>
                    <td style={{ color: "var(--danger)" }}>{hit.error ?? ""}</td>
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
  const history = useAppStore((s) => s.history);
  const toast = useAppStore((s) => s.toast);
  const [issues, setIssues] = useState<import("@/tauri/types").Issue[]>([]);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState<"all" | "critical" | "high" | "medium" | "low" | "info">(
    "all",
  );
  const [selected, setSelected] = useState<string | null>(null);

  const filtered = useMemo(
    () => (filter === "all" ? issues : issues.filter((i) => i.severity === filter)),
    [issues, filter],
  );

  const sevCounts = useMemo(() => {
    const c: Record<string, number> = {
      critical: 0,
      high: 0,
      medium: 0,
      low: 0,
      info: 0,
    };
    issues.forEach((i) => {
      c[i.severity] = (c[i.severity] ?? 0) + 1;
    });
    return c;
  }, [issues]);

  const runScan = async () => {
    setBusy(true);
    try {
      const found = await ScannerApi.scanHistory();
      setIssues(dedupe(found));
      toast("info", `Scan complete — ${found.length} issue${found.length === 1 ? "" : "s"}.`);
    } catch (err) {
      toast("error", `Scan failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const selectedIssue = filtered.find((i) => i.id === selected) ?? filtered[0] ?? null;

  return (
    <SplitPane
      storageKey="target-issues"
      initialSize={0.55}
      first={
        <div className="panel" style={{ height: "100%", borderRadius: 0, border: "none" }}>
          <div className="toolbar">
            <button className="btn primary" onClick={runScan} disabled={busy}>
              {busy ? "Scanning…" : `Run passive scan (${history.length} flows)`}
            </button>
            <select
              value={filter}
              onChange={(e) =>
                setFilter(e.target.value as typeof filter)
              }
              style={{ marginLeft: 8 }}
            >
              <option value="all">All severities ({issues.length})</option>
              <option value="critical">Critical ({sevCounts.critical})</option>
              <option value="high">High ({sevCounts.high})</option>
              <option value="medium">Medium ({sevCounts.medium})</option>
              <option value="low">Low ({sevCounts.low})</option>
              <option value="info">Info ({sevCounts.info})</option>
            </select>
          </div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th style={{ width: 90 }}>Severity</th>
                  <th>Issue</th>
                  <th>Host</th>
                  <th>Path</th>
                </tr>
              </thead>
              <tbody>
                {filtered.length === 0 && (
                  <tr>
                    <td colSpan={4}>
                      <div className="empty-state" style={{ padding: 24 }}>
                        <p>
                          {issues.length === 0
                            ? "Run a passive scan to populate this table."
                            : "No issues match the current filter."}
                        </p>
                      </div>
                    </td>
                  </tr>
                )}
                {filtered.map((issue) => (
                  <tr
                    key={issue.id}
                    onClick={() => setSelected(issue.id)}
                    className={selectedIssue?.id === issue.id ? "row-selected" : ""}
                    style={{ cursor: "pointer" }}
                  >
                    <td>
                      <span className={`status-badge sev-${issue.severity}`}>
                        {issue.severity}
                      </span>
                    </td>
                    <td>{issue.name}</td>
                    <td className="mono">{issue.host}</td>
                    <td className="mono">{issue.path}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      }
      second={
        <div className="panel" style={{ height: "100%", borderRadius: 0, border: "none" }}>
          <div className="panel-header">
            {selectedIssue ? selectedIssue.name : "Issue detail"}
          </div>
          <div className="panel-body" style={{ padding: 12, overflow: "auto" }}>
            {!selectedIssue ? (
              <div className="empty-state">
                <p>Select an issue to inspect details.</p>
              </div>
            ) : (
              <div className="stack" style={{ gap: 10 }}>
                <div>
                  <div className="label">Severity / Confidence</div>
                  <div>
                    <span className={`status-badge sev-${selectedIssue.severity}`}>
                      {selectedIssue.severity}
                    </span>{" "}
                    <span style={{ color: "var(--text-muted)" }}>
                      ({selectedIssue.confidence})
                    </span>
                  </div>
                </div>
                <div>
                  <div className="label">Rule</div>
                  <code className="mono">{selectedIssue.rule_id}</code>
                </div>
                <div>
                  <div className="label">Location</div>
                  <code className="mono">
                    {selectedIssue.host}
                    {selectedIssue.path}
                  </code>
                </div>
                <div>
                  <div className="label">Description</div>
                  <p style={{ margin: 0 }}>{selectedIssue.description}</p>
                </div>
                {selectedIssue.evidence && (
                  <div>
                    <div className="label">Evidence</div>
                    <pre
                      className="code"
                      style={{ whiteSpace: "pre-wrap", padding: 10 }}
                    >
                      {selectedIssue.evidence}
                    </pre>
                  </div>
                )}
                {selectedIssue.remediation && (
                  <div>
                    <div className="label">Remediation</div>
                    <p style={{ margin: 0 }}>{selectedIssue.remediation}</p>
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
      }
    />
  );
}

function dedupe(issues: import("@/tauri/types").Issue[]): import("@/tauri/types").Issue[] {
  const seen = new Set<string>();
  const out: import("@/tauri/types").Issue[] = [];
  for (const issue of issues) {
    const key = `${issue.rule_id}|${issue.host}|${issue.path}|${issue.evidence ?? ""}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(issue);
  }
  return out.sort((a, b) => severityRank(b.severity) - severityRank(a.severity));
}

function severityRank(s: import("@/tauri/types").IssueSeverity): number {
  switch (s) {
    case "critical":
      return 4;
    case "high":
      return 3;
    case "medium":
      return 2;
    case "low":
      return 1;
    default:
      return 0;
  }
}
