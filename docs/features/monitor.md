# Continuous monitoring (Feature AA)

Schedule recurring scans against in-scope targets and surface only the
**new** findings against the previous baseline run.

## Concepts

- **Schedule** — `{ name, target_url, scope_hosts, cadence }`. Cadence is
  one of `hourly`, `daily`, `weekly`.
- **Baseline** — fingerprint set captured from the most recent *successful*
  run. Re-computed only when a run completes without error so a transient
  failure cannot wipe your "what's new" view.
- **Fingerprint** — stable string `{rule_id}|{severity}|{host}|{path}`.
  Same rule on the same endpoint with the same severity is considered the
  same finding across runs.
- **Run record** — per-run report: how many findings are **new**, how many
  are **resolved** (in baseline, missing from this run), how many are
  **still present**.

## Persistence

Schedules and run records are JSON-serialised to
`~/.nyxproxy/monitor.json` after every mutation. The store is bounded at
200 historical runs to keep the file small.

## API

| Tauri command                 | Purpose                                                   |
| ----------------------------- | --------------------------------------------------------- |
| `monitor_upsert_cmd`          | Add or replace a schedule (returns the persisted record). |
| `monitor_list_cmd`            | List all schedules sorted by `created_at`.                |
| `monitor_remove_cmd`          | Delete a schedule by id.                                  |
| `monitor_complete_run_cmd`    | Record a finished run; updates baseline + `next_run_at`.  |
| `monitor_runs_cmd`            | List all run records (most recent last).                  |

## UI

Sidebar → **Monitor**. Add a schedule with name + target URL + cadence,
view the live table of schedules, and scroll through the last 50 runs with
their new/resolved/still-present counts.

## Tests

`apps/desktop/crates/nyxproxy-core/src/monitor.rs` ships six tests:
cadence-interval correctness, `is_due` gating, first-run treats everything
as new, second-run diffs against baseline, errored runs preserve baseline,
fingerprint stability, run-history bounding at 200.
