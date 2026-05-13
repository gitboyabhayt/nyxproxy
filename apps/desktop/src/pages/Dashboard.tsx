import { useMemo } from "react";
import { ArrowRight, Brain, Crosshair, Repeat, Shield } from "lucide-react";

import { statusBucket } from "@/lib/codec";
import { useAppStore } from "@/state/store";

interface Props {
  onNavigate: (page: any) => void;
}

export function DashboardPage({ onNavigate }: Props) {
  const history = useAppStore((s) => s.history);
  const proxyStatus = useAppStore((s) => s.proxy.status);
  const ca = useAppStore((s) => s.ca);
  const providers = useAppStore((s) => s.providers);

  const stats = useMemo(() => {
    const total = history.length;
    const buckets: Record<string, number> = {};
    let uniqueHosts = new Set<string>();
    let errors = 0;
    let totalMs = 0;
    let timed = 0;
    for (const entry of history) {
      const status = entry.flow.response?.status ?? 0;
      const bucket = statusBucket(status);
      buckets[bucket] = (buckets[bucket] ?? 0) + 1;
      uniqueHosts.add(entry.flow.request.authority);
      if (entry.flow.error) errors++;
      if (entry.flow.response) {
        totalMs += entry.flow.response.elapsed_ms;
        timed++;
      }
    }
    return {
      total,
      uniqueHosts: uniqueHosts.size,
      avgMs: timed === 0 ? 0 : Math.round(totalMs / timed),
      buckets,
      errors,
    };
  }, [history]);

  const recent = history.slice(0, 8);
  const availableProviders = providers?.providers.filter((p) => p.available).length ?? 0;

  return (
    <div className="section" style={{ overflow: "auto", flex: 1 }}>
      <div>
        <h2>Welcome to NyxProxy</h2>
        <p style={{ color: "var(--text-dim)", marginTop: 4 }}>
          A fast, AI-driven open-source alternative to Burp Suite. Start the proxy, point your browser at it, and capture
          live traffic — then send anything to Repeater, Intruder, Decoder, or our AI assistant.
        </p>
      </div>

      <div className="cards">
        <div className="card">
          <div className="label">Flows captured</div>
          <div className="value">{stats.total}</div>
          <div className="sub">{stats.uniqueHosts} hosts seen</div>
        </div>
        <div className="card">
          <div className="label">Avg latency</div>
          <div className="value">{stats.avgMs} ms</div>
          <div className="sub">Across {history.filter((h) => h.flow.response).length} responses</div>
        </div>
        <div className="card">
          <div className="label">Errors</div>
          <div className="value" style={{ color: stats.errors > 0 ? "var(--danger)" : "var(--success)" }}>
            {stats.errors}
          </div>
          <div className="sub">Upstream failures</div>
        </div>
        <div className="card">
          <div className="label">AI providers</div>
          <div className="value">{availableProviders}</div>
          <div className="sub">of {providers?.providers.length ?? 0} configured</div>
        </div>
      </div>

      <div className="cards" style={{ gridTemplateColumns: "repeat(4, 1fr)" }}>
        {[
          { id: "proxy", icon: Shield, label: "Open Proxy", sub: "Intercept and inspect HTTP/S traffic" },
          { id: "repeater", icon: Repeat, label: "Open Repeater", sub: "Clone, edit, and resend any request" },
          { id: "intruder", icon: Crosshair, label: "Open Intruder", sub: "Sniper attack with custom payloads" },
          { id: "ai", icon: Brain, label: "AI Assistant", sub: "Explain, find vulns, generate payloads" },
        ].map((q) => {
          const Icon = q.icon;
          return (
            <div
              key={q.id}
              className="card"
              style={{ cursor: "pointer" }}
              onClick={() => onNavigate(q.id)}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <Icon size={18} />
                <strong style={{ flex: 1 }}>{q.label}</strong>
                <ArrowRight size={14} />
              </div>
              <div className="sub" style={{ marginTop: 8 }}>{q.sub}</div>
            </div>
          );
        })}
      </div>

      <div className="panel" style={{ marginTop: 4 }}>
        <div className="panel-header">
          Recent flows
          <span style={{ marginLeft: "auto", color: "var(--text-muted)" }}>
            CA: {ca?.cert_path ?? "—"}
          </span>
        </div>
        <div className="panel-body" style={{ overflow: "auto", maxHeight: 280 }}>
          {recent.length === 0 ? (
            <div className="empty-state">
              <h3>No traffic captured yet</h3>
              <p>
                Click <strong>Start proxy</strong> in the top bar, then configure your browser to use
                <code className="code" style={{ display: "inline", padding: "0 4px" }}>
                  {proxyStatus?.listen_addr ?? "127.0.0.1:8089"}
                </code>{" "}
                as an HTTP proxy. Install the NyxProxy root CA from User options → Certificates to intercept HTTPS.
              </p>
            </div>
          ) : (
            <table className="data-table">
              <thead>
                <tr>
                  <th>Method</th>
                  <th>Host</th>
                  <th>Path</th>
                  <th>Status</th>
                  <th>Length</th>
                  <th>Latency</th>
                </tr>
              </thead>
              <tbody>
                {recent.map((entry) => {
                  const status = entry.flow.response?.status ?? 0;
                  return (
                    <tr key={entry.flow.id}>
                      <td>
                        <span className={`pill method-${entry.flow.request.method}`}>
                          {entry.flow.request.method}
                        </span>
                      </td>
                      <td>{entry.flow.request.authority}</td>
                      <td style={{ maxWidth: 260, overflow: "hidden", textOverflow: "ellipsis" }}>
                        {entry.flow.request.path}
                      </td>
                      <td>
                        <span className={`status-badge ${statusBucket(status)}`}>{status || "–"}</span>
                      </td>
                      <td>{entry.flow.response?.body_size ?? "—"}</td>
                      <td>{entry.flow.response?.elapsed_ms ?? "—"} ms</td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}
