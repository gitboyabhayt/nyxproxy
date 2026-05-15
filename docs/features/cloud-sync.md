# Cloud sync (Supabase) — Feature F

NyxProxy supports an **opt-in** cloud workspace sync built on
[Supabase](https://supabase.com)'s REST/PostgREST API. This lets you push your
local workspace (settings snapshot, repeater history index, scope, scanner
findings index, etc.) into a Postgres row keyed by `(owner, workspace_id)` and
pull it back from a second machine.

The sync layer is **not enabled by default**. The hosted reference backend at
`https://nyxproxy-backend.onrender.com` ships with sync disabled — the only
thing you can do is `GET /sync/status`, which returns
`{"enabled": false, "provider": null}` so the desktop UI can show the "not
configured" notice without surprising the user with a wall of HTTP 500s.

## Why Supabase?

We needed a backend that:

1. Has a no-cost free tier (so casual users can opt in without a credit
   card).
2. Speaks plain REST/JSON (so we don't have to embed a Postgres driver in
   the FastAPI process — `httpx.AsyncClient` is enough).
3. Supports upserts with optimistic concurrency (`Prefer: resolution=
   merge-duplicates` on the PostgREST `/rest/v1/<table>` endpoint).

Supabase ticked all three. The implementation lives in
[`apps/backend/nyxproxy_backend/routes/sync.py`](../../apps/backend/nyxproxy_backend/routes/sync.py).
Swap providers later by writing another `_SupabaseClient`-style class — the
public API (`/sync/status`, `/sync/push`, `/sync/pull/{owner}/{id}`,
`DELETE /sync/{owner}/{id}`) stays stable.

## Setup (self-host operator)

1. Create a Supabase project (the free tier is more than enough for personal
   use — 500 MB Postgres, 2 GB egress / month).
2. In the SQL editor, run:

   ```sql
   create table if not exists public.nyx_workspaces (
     id text not null,
     owner text not null,
     revision bigint not null default 1,
     payload jsonb not null default '{}',
     updated_at timestamptz not null default now(),
     primary key (id, owner)
   );
   create index if not exists nyx_workspaces_owner_idx on public.nyx_workspaces (owner);
   alter table public.nyx_workspaces enable row level security;
   -- For single-user self-hosting, full public read/write is fine because the
   -- gateway in front of the table (NyxProxy backend) is the only caller.
   create policy "nyx_workspaces full" on public.nyx_workspaces for all using (true) with check (true);
   ```

3. Copy your Supabase **project URL** and **service role key** (Settings →
   API). The service role key bypasses RLS — guard it carefully and never
   ship it to the desktop client.

4. Set the env vars on the backend deployment:

   ```bash
   SUPABASE_URL=https://xxxxxxxxxxxx.supabase.co
   SUPABASE_SERVICE_KEY=eyJhbGc...   # service role
   ```

5. Redeploy. `/sync/status` now returns `{"enabled": true, "provider": "supabase"}`.

## Setup (NyxProxy desktop user)

1. Open **User options → Cloud sync**.
2. If the panel says "not configured", talk to whoever runs your backend
   (yourself, if you self-host).
3. Otherwise, fill in your `owner` (an email address or stable UUID — it's
   the partitioning key, not authentication) and a workspace ID (e.g.
   `default`, `prod-pentest`, `client-acme`).
4. Click **Push now**.
5. On a second machine, install NyxProxy, point it at the same backend,
   open User options → Cloud sync, fill in the same `owner` + workspace ID,
   click **Pull from cloud**.

## Optimistic concurrency

Every push carries a `revision` integer. If the client provides
`expected_revision` and the server already has a higher revision (another
device pushed in the meantime), the server returns **HTTP 409 Conflict** with
`{"error": "revision_conflict", "actual_revision": N}`. The client is
expected to `pull` to fetch the newer revision, merge locally, and retry.

This is intentionally simpler than CRDTs — NyxProxy workspaces are mostly
"settings + history index" rather than collaborative text, so a
last-writer-wins-with-warning policy is fine.

## Authentication

The `/sync/*` endpoints sit behind the same `BACKEND_API_TOKEN` gate as the
rest of the backend (see `apps/backend/nyxproxy_backend/main.py::_enforce_token`).
If your backend has a bearer token set, the desktop client sends it as
`Authorization: Bearer …` (configured under User options → Backend bearer
token).

## Tests

Round-trip + conflict + feature-disabled behaviour are covered by
[`apps/backend/tests/test_sync.py`](../../apps/backend/tests/test_sync.py).
Supabase REST calls are mocked with `respx` so the suite is hermetic.

## Files

| File | Purpose |
| ---- | ------- |
| `apps/backend/nyxproxy_backend/routes/sync.py` | FastAPI router, Supabase client |
| `apps/backend/tests/test_sync.py`              | Unit tests (respx-mocked) |
| `apps/desktop/src/tauri/api.ts` → `SyncApi`    | Typed HTTP wrapper |
| `apps/desktop/src/pages/UserOptions.tsx` → `CloudSyncPanel` | UI |
