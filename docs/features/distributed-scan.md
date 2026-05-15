# Distributed scanning fleet — Feature K

NyxProxy can horizontally scale its passive scanner across an arbitrary
number of worker processes that long-poll a backend job queue. Use this when
you have hundreds of URLs to scan, multiple network egresses to spread the
load across, or simply want to keep the desktop UI responsive while the
heavy lifting happens elsewhere.

```
+----------------+   POST /scan/jobs            +-----------------+
|  Desktop UI    | ---------------------------> |     Backend     |
|  (distributor) |                              |                 |
+----------------+                              |  SQLite queue   |
                                                |  scan_jobs.db   |
                                                |                 |
+----------------+   GET /scan/jobs/next        |                 |
|  Worker #1     | <--------------------------- |                 |
|  Worker #2     |   (long-poll, atomic claim)  |                 |
|  Worker #N     | <--------------------------- |                 |
+----------------+   POST /jobs/{id}/result --> +-----------------+
```

## Backend

The job queue is a single SQLite table — small, durable, no extra
infrastructure. Schema (in `apps/backend/nyxproxy_backend/routes/scan_jobs.py`):

```sql
CREATE TABLE IF NOT EXISTS scan_jobs (
  id           TEXT PRIMARY KEY,
  target       TEXT NOT NULL,    -- JSON: {url, method, headers, body_b64}
  rules        TEXT NOT NULL,    -- JSON: ["xss", "sqli", ...] or []
  label        TEXT,
  status       TEXT NOT NULL,    -- queued | in_progress | done | failed
  worker_id    TEXT,
  created_at   REAL NOT NULL,
  started_at   REAL,
  completed_at REAL,
  result       TEXT              -- JSON: {findings, error, elapsed_ms}
);
```

The DB path is configurable via `NYXPROXY_SCAN_JOBS_DB`. Defaults to
`scan_jobs.db` next to the backend process.

Endpoints (all behind `BACKEND_API_TOKEN`):

| Method | Path                              | Purpose |
| ------ | --------------------------------- | ------- |
| POST   | `/scan/jobs`                      | Enqueue one or more jobs |
| GET    | `/scan/jobs`                      | List jobs (optional `?status=…` filter) |
| GET    | `/scan/jobs/{id}`                 | Fetch a single job |
| GET    | `/scan/jobs/next?worker_id=…&wait=N` | Long-poll for the next job, atomically claim |
| POST   | `/scan/jobs/{id}/result?worker_id=…` | Submit a result, marks job `done` (or `failed` if `error` set) |
| DELETE | `/scan/jobs?status=done`          | Clear completed jobs |

Atomic claim is implemented via `UPDATE scan_jobs SET status='in_progress'`
gated on `WHERE status='queued'` with a row-count check. Long-polling sleeps
for 500 ms between attempts up to the requested `wait` seconds (default 25,
hard-capped at 60).

## Worker

The worker binary lives in
[`apps/desktop/crates/nyxproxy-worker`](../../apps/desktop/crates/nyxproxy-worker)
and is built as part of the desktop workspace (`cargo build -p
nyxproxy-worker --release`). It's deliberately a separate binary from the
Tauri shell — it has no GUI dependencies (no GTK, no WebKit) and runs
anywhere `tokio + reqwest + rustls` works.

Configuration (all env vars):

| Env var              | Default                    | Purpose |
| -------------------- | -------------------------- | ------- |
| `NYX_BACKEND_URL`    | _(required)_               | Backend base URL |
| `NYX_BACKEND_TOKEN`  | unset                      | Optional bearer token |
| `NYX_WORKER_ID`      | `worker-<8-hex>` (random)  | Identifier surfaced in the desktop UI |
| `NYX_POLL_TIMEOUT`   | `25`                       | Long-poll seconds per attempt |

Run it like:

```bash
export NYX_BACKEND_URL=https://nyxproxy-backend.onrender.com
export NYX_BACKEND_TOKEN=$BACKEND_TOKEN
export NYX_WORKER_ID=worker-edge-london
cargo run -p nyxproxy-worker --release
```

The worker:

1. Long-polls `GET /scan/jobs/next` with its `worker_id`.
2. When it gets a job, decodes the target, makes the HTTP request, captures
   the response (headers + body) into an `HttpFlow`.
3. Runs the deterministic passive scanner (`nyxproxy_core::scanner::scan`)
   over the flow.
4. Optionally filters findings by the job's `rules` allowlist.
5. POSTs `{findings, error, elapsed_ms}` back to `/scan/jobs/{id}/result`.
6. Loops.

On error, the worker logs and backs off exponentially (500 ms → 15 s cap) so
a transient backend outage doesn't melt the queue.

## Desktop UI

The **Distributed scan** page (`apps/desktop/src/pages/DistributedScan.tsx`)
provides:

* A textarea for one URL per line.
* A shard slider (1–64) that round-robins URLs across worker labels.
* A rule filter — pick the passive scanner rules you care about, or leave
  blank to run everything.
* Live job status (auto-polls every 4 s): counts per status, list of active
  workers, table of jobs with status / target / worker / findings count /
  elapsed time.
* "Clear completed" to GC the `done` rows once you've consumed the findings.

The shard algorithm (in `DistributedScanPage::shardTargets`) is
**round-robin interleaving**, not contiguous chunking. This prevents a slow
target near the front of the input list from concentrating all the latency
on a single shard.

## Tests

[`apps/backend/tests/test_scan_jobs.py`](../../apps/backend/tests/test_scan_jobs.py)
covers:

* Enqueue → list returns the queued rows.
* Worker A claims the next job, worker B gets nothing (atomic claim).
* Submitting a result marks the job `done`.
* `DELETE /scan/jobs?status=done` clears completed jobs and leaves the
  others alone.

The fixture isolates SQLite to `tmp_path / "scan_jobs.db"` per test, so the
suite is hermetic and parallel-safe.

## Files

| File | Purpose |
| ---- | ------- |
| `apps/backend/nyxproxy_backend/routes/scan_jobs.py` | FastAPI routes + SQLite queue |
| `apps/backend/tests/test_scan_jobs.py` | Queue state machine tests |
| `apps/desktop/crates/nyxproxy-worker/src/main.rs` | Worker binary |
| `apps/desktop/crates/nyxproxy-worker/Cargo.toml`  | Worker crate manifest |
| `apps/desktop/src/tauri/api.ts` → `ScanFleetApi`  | Typed HTTP wrapper |
| `apps/desktop/src/pages/DistributedScan.tsx`      | Distribute scan UI |
