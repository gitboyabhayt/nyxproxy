import { useEffect, useState } from "react";

import { OwaspDashboardApi, type OwaspDashboard, invoke } from "@/tauri/api";
import type { Issue } from "@/tauri/types";
import { useAppStore } from "@/state/store";

export function OwaspDashboardPage() {
  const toast = useAppStore((s) => s.toast);
  const [dashboard, setDashboard] = useState<OwaspDashboard | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh(): Promise<void> {
    setBusy(true);
    try {
      const issues = await invoke<Issue[]>("scanner_scan_history");
      const d = await OwaspDashboardApi.build(issues);
      setDashboard(d);
    } catch (err) {
      toast("error", `Could not build dashboard: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <h1>OWASP Top-10 dashboard</h1>
          <p>
            Live distribution of your findings vs the OWASP 2021 + Verizon
            DBIR 2024 industry baseline. Positive delta = over-represented in
            your codebase relative to the public dataset.
          </p>
        </div>
        <div>
          <button onClick={() => void refresh()} disabled={busy}>
            {busy ? "Refreshing…" : "Refresh"}
          </button>
        </div>
      </header>

      {!dashboard ? (
        <p className="muted">Loading…</p>
      ) : (
        <section className="panel">
          <h2>Total findings: {dashboard.total}</h2>
          <table className="data-table">
            <thead>
              <tr>
                <th>Code</th>
                <th>Category</th>
                <th>Count</th>
                <th>You %</th>
                <th>Industry %</th>
                <th>Δ pp</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {dashboard.categories.map((c) => (
                <tr key={c.code}>
                  <td className="mono">{c.code}</td>
                  <td>{c.title}</td>
                  <td>{c.count}</td>
                  <td>{c.percent.toFixed(1)}%</td>
                  <td>{c.industryBaseline.toFixed(1)}%</td>
                  <td
                    style={{
                      color:
                        c.deltaPp > 5
                          ? "#f55"
                          : c.deltaPp < -2
                            ? "#5f8"
                            : undefined,
                      fontWeight: 600,
                    }}
                  >
                    {c.deltaPp >= 0 ? "+" : ""}
                    {c.deltaPp.toFixed(1)}
                  </td>
                  <td style={{ minWidth: 160 }}>
                    <div
                      style={{
                        height: 8,
                        background: "#1f2937",
                        borderRadius: 4,
                        position: "relative",
                        overflow: "hidden",
                      }}
                    >
                      <div
                        style={{
                          position: "absolute",
                          left: 0,
                          top: 0,
                          bottom: 0,
                          width: `${Math.min(100, c.percent)}%`,
                          background: "#3b82f6",
                        }}
                      />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
    </div>
  );
}
