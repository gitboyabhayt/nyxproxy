import { useMemo, useState } from "react";

import { SplitPane } from "@/components/SplitPane";
import { RequestViewer } from "@/components/RequestViewer";
import { formatBytes, statusBucket } from "@/lib/codec";
import { useAppStore } from "@/state/store";

export function LoggerPage() {
  const history = useAppStore((s) => s.history);
  const [filter, setFilter] = useState("");
  const [statusBucketFilter, setStatusBucketFilter] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const filtered = useMemo(() => {
    const q = filter.toLowerCase();
    return history.filter((entry) => {
      if (statusBucketFilter) {
        const bucket = statusBucket(entry.flow.response?.status ?? 0);
        if (bucket !== statusBucketFilter) return false;
      }
      if (!q) return true;
      return (
        entry.flow.request.url.toLowerCase().includes(q) ||
        entry.flow.request.method.toLowerCase().includes(q) ||
        entry.flow.request.authority.toLowerCase().includes(q) ||
        (entry.note ?? "").toLowerCase().includes(q)
      );
    });
  }, [history, filter, statusBucketFilter]);

  const selected = filtered.find((e) => e.flow.id === selectedId) ?? filtered[0];

  return (
    <>
      <div className="toolbar">
        <input
          placeholder="Search every captured flow…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          className="grow"
        />
        <select value={statusBucketFilter} onChange={(e) => setStatusBucketFilter(e.target.value)}>
          <option value="">Any status</option>
          <option value="status-2xx">2xx</option>
          <option value="status-3xx">3xx</option>
          <option value="status-4xx">4xx</option>
          <option value="status-5xx">5xx</option>
        </select>
      </div>
      <SplitPane
        storageKey="logger"
        direction="vertical"
        initialSize={0.5}
        first={
          <div style={{ overflow: "auto", height: "100%" }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th>Method</th>
                  <th>Host</th>
                  <th>URL</th>
                  <th>Status</th>
                  <th>Length</th>
                  <th>Time</th>
                  <th>Note</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((entry) => (
                  <tr
                    key={entry.flow.id}
                    className={selected?.flow.id === entry.flow.id ? "selected" : ""}
                    onClick={() => setSelectedId(entry.flow.id)}
                  >
                    <td>
                      <span className={`pill method-${entry.flow.request.method}`}>
                        {entry.flow.request.method}
                      </span>
                    </td>
                    <td>{entry.flow.request.authority}</td>
                    <td
                      style={{
                        maxWidth: 360,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {entry.flow.request.path}
                    </td>
                    <td>
                      <span className={`status-badge ${statusBucket(entry.flow.response?.status ?? 0)}`}>
                        {entry.flow.response?.status ?? "—"}
                      </span>
                    </td>
                    <td>{entry.flow.response ? formatBytes(entry.flow.response.body_size) : "—"}</td>
                    <td>{entry.flow.response?.elapsed_ms ?? "—"} ms</td>
                    <td style={{ color: "var(--text-muted)" }}>{entry.note ?? ""}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        }
        second={
          selected ? (
            <SplitPane
              storageKey="logger-detail"
              initialSize={0.5}
              first={
                <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
                  <div className="panel-header">Request</div>
                  <RequestViewer side="request" request={selected.flow.request} />
                </div>
              }
              second={
                <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
                  <div className="panel-header">Response</div>
                  <RequestViewer side="response" response={selected.flow.response} />
                </div>
              }
            />
          ) : (
            <div className="empty-state">
              <h3>Nothing captured yet</h3>
              <p>The Logger shows every flow ever observed by the proxy, with full filtering and search.</p>
            </div>
          )
        }
      />
    </>
  );
}
