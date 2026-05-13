import { useState } from "react";
import { useAppStore } from "@/state/store";

export function ProjectOptionsPage() {
  const config = useAppStore((s) => s.proxy.config);
  const save = useAppStore((s) => s.saveProxyConfig);
  const clearHistory = useAppStore((s) => s.clearHistory);
  const [pendingAddr, setPendingAddr] = useState<string | null>(null);

  if (!config) return <div className="banner">Loading project options…</div>;

  return (
    <div className="section" style={{ overflow: "auto" }}>
      <div>
        <h2>Project options</h2>
        <p style={{ color: "var(--text-dim)" }}>
          Settings stored alongside the current project. NyxProxy currently runs in single-project mode; full
          project/workspace switching ships in Phase 2.
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
