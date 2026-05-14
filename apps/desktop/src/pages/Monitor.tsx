import { useEffect, useState } from "react";

import {
  MonitorApi,
  type Cadence,
  type MonitorRunRecord,
  type MonitorSchedule,
} from "@/tauri/api";
import { useAppStore } from "@/state/store";

const CADENCES: { id: Cadence; label: string }[] = [
  { id: "hourly", label: "Every hour" },
  { id: "daily", label: "Every day" },
  { id: "weekly", label: "Every week" },
];

export function MonitorPage() {
  const toast = useAppStore((s) => s.toast);
  const [schedules, setSchedules] = useState<MonitorSchedule[]>([]);
  const [runs, setRuns] = useState<MonitorRunRecord[]>([]);
  const [name, setName] = useState("");
  const [target, setTarget] = useState("");
  const [scopeHosts, setScopeHosts] = useState("");
  const [cadence, setCadence] = useState<Cadence>("daily");
  const [busy, setBusy] = useState(false);

  async function refresh(): Promise<void> {
    try {
      const [s, r] = await Promise.all([MonitorApi.list(), MonitorApi.runs()]);
      setSchedules(s);
      setRuns(r);
    } catch (err) {
      toast("error", `Could not load monitors: ${err}`);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function add(): Promise<void> {
    if (!name.trim() || !target.trim()) {
      toast("error", "Name and target URL are required");
      return;
    }
    setBusy(true);
    try {
      const hosts = scopeHosts
        .split(/[,\s]+/)
        .map((h) => h.trim())
        .filter(Boolean);
      await MonitorApi.upsert({
        name: name.trim(),
        targetUrl: target.trim(),
        scopeHosts: hosts,
        cadence,
      });
      setName("");
      setTarget("");
      setScopeHosts("");
      toast("info", `Scheduled "${name}"`);
      await refresh();
    } catch (err) {
      toast("error", `Could not add schedule: ${err}`);
    } finally {
      setBusy(false);
    }
  }

  async function remove(id: string): Promise<void> {
    try {
      await MonitorApi.remove(id);
      await refresh();
    } catch (err) {
      toast("error", `Remove failed: ${err}`);
    }
  }

  return (
    <div className="page">
      <header className="page-header">
        <div>
          <h1>Continuous monitoring</h1>
          <p>
            Schedule recurring scans against in-scope targets. New issues are
            highlighted against the previous baseline run.
          </p>
        </div>
      </header>

      <section className="panel">
        <h2>New schedule</h2>
        <div className="form-grid">
          <label>
            Name
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Production API"
              disabled={busy}
            />
          </label>
          <label>
            Target URL
            <input
              type="text"
              value={target}
              onChange={(e) => setTarget(e.target.value)}
              placeholder="https://api.example.com/"
              disabled={busy}
            />
          </label>
          <label>
            Scope hosts (comma separated, optional)
            <input
              type="text"
              value={scopeHosts}
              onChange={(e) => setScopeHosts(e.target.value)}
              placeholder="api.example.com, www.example.com"
              disabled={busy}
            />
          </label>
          <label>
            Cadence
            <select
              value={cadence}
              onChange={(e) => setCadence(e.target.value as Cadence)}
              disabled={busy}
            >
              {CADENCES.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        <button onClick={add} disabled={busy}>
          Add schedule
        </button>
      </section>

      <section className="panel">
        <h2>Active schedules ({schedules.length})</h2>
        {schedules.length === 0 ? (
          <p className="muted">No schedules yet.</p>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Target</th>
                <th>Cadence</th>
                <th>Last run</th>
                <th>Next run</th>
                <th>Baseline</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {schedules.map((s) => (
                <tr key={s.id}>
                  <td>{s.name}</td>
                  <td className="mono">{s.targetUrl}</td>
                  <td>{s.cadence}</td>
                  <td>
                    {s.lastRunAt
                      ? new Date(s.lastRunAt).toLocaleString()
                      : "—"}
                  </td>
                  <td>{new Date(s.nextRunAt).toLocaleString()}</td>
                  <td>{s.baselineFingerprints.length} findings</td>
                  <td>
                    <button onClick={() => void remove(s.id)}>Remove</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className="panel">
        <h2>Run history ({runs.length})</h2>
        {runs.length === 0 ? (
          <p className="muted">No runs recorded yet.</p>
        ) : (
          <ul className="run-list">
            {runs
              .slice()
              .reverse()
              .slice(0, 50)
              .map((r, i) => (
                <li key={`${r.scheduleId}-${i}`}>
                  <div>
                    <strong>{new Date(r.finishedAt).toLocaleString()}</strong>{" "}
                    — <code className="mono">{r.scheduleId.slice(0, 8)}</code>
                  </div>
                  <div>
                    {r.error ? (
                      <span style={{ color: "#f55" }}>error: {r.error}</span>
                    ) : (
                      <>
                        new: <strong>{r.newIssues.length}</strong>, resolved:{" "}
                        <strong>{r.resolvedIssues.length}</strong>, still
                        present: <strong>{r.stillPresent}</strong>
                      </>
                    )}
                  </div>
                </li>
              ))}
          </ul>
        )}
      </section>
    </div>
  );
}
