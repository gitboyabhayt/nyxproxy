import { useEffect, useMemo, useState } from "react";
import { Layers, ListChecks, Trash2 } from "lucide-react";

import { DEFAULT_BACKEND_URL } from "@/lib/backend";
import { useAppStore } from "@/state/store";
import { ScanFleetApi, type ScanJob, type ScanJobInput } from "@/tauri/api";

const RULE_OPTIONS = [
  "xss",
  "sqli",
  "ssrf",
  "lfi",
  "open_redirect",
  "cors",
  "secrets_in_response",
  "missing_security_headers",
];

const STATUS_COLOURS: Record<ScanJob["status"], string> = {
  queued: "var(--text-dim)",
  in_progress: "var(--warning)",
  done: "var(--success)",
  failed: "var(--danger)",
};

export function DistributedScanPage() {
  const toast = useAppStore((s) => s.toast);
  const settings = useAppStore((s) => s.settings);
  const backendUrl = settings?.backend_url || DEFAULT_BACKEND_URL;
  const token = settings?.backend_token ?? undefined;

  const [targetsRaw, setTargetsRaw] = useState<string>("");
  const [selectedRules, setSelectedRules] = useState<Set<string>>(new Set());
  const [label, setLabel] = useState<string>("");
  const [shards, setShards] = useState<number>(4);
  const [jobs, setJobs] = useState<ScanJob[]>([]);
  const [busy, setBusy] = useState(false);

  const refresh = async () => {
    try {
      setJobs(await ScanFleetApi.list(backendUrl, token));
    } catch (err) {
      toast("error", `Job list failed: ${err}`);
    }
  };

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 4000);
    return () => clearInterval(interval);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [backendUrl, token]);

  const toggleRule = (r: string) =>
    setSelectedRules((prev) => {
      const next = new Set(prev);
      if (next.has(r)) next.delete(r);
      else next.add(r);
      return next;
    });

  const parsedTargets = useMemo(() => {
    return targetsRaw
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0);
  }, [targetsRaw]);

  /**
   * Shard a list of targets across N worker slots. Targets are interleaved
   * (round-robin) rather than chunked so that a slow target near the front
   * doesn't drag down a single shard's whole batch — every shard ends up
   * with a mix.
   */
  const shardTargets = (targets: string[], slots: number): string[][] => {
    const out: string[][] = Array.from({ length: slots }, () => []);
    targets.forEach((t, i) => {
      const shard = out[i % slots];
      if (shard) shard.push(t);
    });
    return out;
  };

  const distribute = async () => {
    if (parsedTargets.length === 0) {
      toast("error", "Paste at least one target URL.");
      return;
    }
    setBusy(true);
    try {
      const shardCount = Math.max(1, Math.min(64, shards));
      const sharded = shardTargets(parsedTargets, shardCount);
      const jobsIn: ScanJobInput[] = [];
      sharded.forEach((shard, shardIndex) => {
        shard.forEach((url) => {
          jobsIn.push({
            target: { url, method: "GET", headers: {} },
            rules: Array.from(selectedRules),
            label: label ? `${label} #${shardIndex + 1}` : `shard #${shardIndex + 1}`,
          });
        });
      });
      const res = await ScanFleetApi.enqueue(backendUrl, jobsIn, token);
      toast(
        "info",
        `Enqueued ${res.ids.length} job(s) across ${shardCount} worker shard(s).`,
      );
      setTargetsRaw("");
      await refresh();
    } catch (err) {
      toast("error", `Distribute failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const clearCompleted = async () => {
    try {
      const res = await ScanFleetApi.clear(backendUrl, token, "done");
      toast("info", `Cleared ${res.deleted} completed job(s).`);
      await refresh();
    } catch (err) {
      toast("error", `Clear failed: ${err}`);
    }
  };

  const stats = useMemo(() => {
    const out = { queued: 0, in_progress: 0, done: 0, failed: 0 };
    for (const j of jobs) out[j.status]++;
    return out;
  }, [jobs]);

  const workers = useMemo(() => {
    const m = new Map<string, number>();
    for (const j of jobs) {
      if (!j.worker_id) continue;
      m.set(j.worker_id, (m.get(j.worker_id) ?? 0) + 1);
    }
    return Array.from(m.entries()).sort();
  }, [jobs]);

  return (
    <>
      <div className="toolbar" style={{ gap: 8 }}>
        <button className="btn primary" onClick={distribute} disabled={busy}>
          <Layers size={14} /> Distribute scan
        </button>
        <button className="btn ghost" onClick={refresh}>
          <ListChecks size={14} /> Refresh
        </button>
        <button className="btn danger" onClick={clearCompleted}>
          <Trash2 size={14} /> Clear completed
        </button>
        <span style={{ flex: 1 }} />
        <span className="muted" style={{ fontSize: 11 }}>
          Backend: {backendUrl}
        </span>
      </div>

      <div className="row-wrap" style={{ padding: "0 0 8px", gap: 12 }}>
        <span className="chip">queued: {stats.queued}</span>
        <span className="chip">in_progress: {stats.in_progress}</span>
        <span className="chip">done: {stats.done}</span>
        <span className="chip">failed: {stats.failed}</span>
        <span className="chip">workers: {workers.length}</span>
      </div>

      <div
        className="main-content"
        style={{ display: "flex", gap: 12, flexWrap: "wrap", overflow: "hidden" }}
      >
        <div className="panel" style={{ flex: "1 1 320px", minWidth: 280 }}>
          <div className="panel-header">Targets</div>
          <div
            className="panel-body"
            style={{ display: "flex", flexDirection: "column", gap: 8, padding: 10 }}
          >
            <input
              placeholder="Job label (optional, e.g. 'prod sweep')"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              style={{ width: "100%" }}
            />
            <label className="muted" style={{ fontSize: 11 }}>
              Worker shards (1–64):{" "}
              <input
                type="number"
                min={1}
                max={64}
                value={shards}
                onChange={(e) => setShards(parseInt(e.target.value || "1", 10))}
                style={{ width: 80 }}
              />
            </label>
            <textarea
              rows={8}
              placeholder={`One URL per line, e.g.\nhttps://target.example.com/api/users\nhttps://target.example.com/api/orders`}
              value={targetsRaw}
              onChange={(e) => setTargetsRaw(e.target.value)}
              style={{ width: "100%", fontFamily: "var(--font-mono)" }}
            />
            <div className="muted" style={{ fontSize: 11 }}>
              {parsedTargets.length} URL{parsedTargets.length === 1 ? "" : "s"} parsed.
            </div>
            <div>
              <div className="muted" style={{ fontSize: 11, marginBottom: 4 }}>
                Rule filter (empty = run every scanner rule):
              </div>
              <div className="row-wrap">
                {RULE_OPTIONS.map((r) => (
                  <span
                    key={r}
                    className={`chip ${selectedRules.has(r) ? "chip-active" : ""}`}
                    onClick={() => toggleRule(r)}
                    style={{ cursor: "pointer" }}
                  >
                    {r}
                  </span>
                ))}
              </div>
            </div>
          </div>
        </div>

        <div className="panel" style={{ flex: "2 1 420px", minWidth: 320, overflow: "hidden", display: "flex", flexDirection: "column" }}>
          <div className="panel-header">Jobs ({jobs.length})</div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            <table className="table responsive-table">
              <thead>
                <tr>
                  <th>Status</th>
                  <th>Target</th>
                  <th>Worker</th>
                  <th>Findings</th>
                  <th>Elapsed</th>
                </tr>
              </thead>
              <tbody>
                {jobs.length === 0 ? (
                  <tr>
                    <td colSpan={5} className="muted" style={{ textAlign: "center", padding: 16 }}>
                      No jobs yet. Add some target URLs and click "Distribute scan".
                    </td>
                  </tr>
                ) : (
                  jobs.map((j) => (
                    <tr key={j.id}>
                      <td>
                        <span
                          className="badge"
                          style={{ color: STATUS_COLOURS[j.status] }}
                        >
                          {j.status}
                        </span>
                      </td>
                      <td className="mono" style={{ fontSize: 11, wordBreak: "break-all" }}>
                        {j.target.method} {j.target.url}
                      </td>
                      <td className="mono" style={{ fontSize: 11 }}>
                        {j.worker_id ?? "—"}
                      </td>
                      <td>{j.result?.findings?.length ?? 0}</td>
                      <td>{j.result?.elapsed_ms != null ? `${j.result.elapsed_ms} ms` : "—"}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </>
  );
}
