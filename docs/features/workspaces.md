# Project workspaces (.nyxproxy)

Workspaces let you persist the full state of an engagement — name, notes, in-scope hosts, captured history, and scanner issues — into a single portable file you can move between machines or hand off to a teammate.

## File format

```
+---------+--------+---------+
| 6 bytes | 2B LE  | payload |
| NYXPRJ  | u16 v  | zstd JSON|
+---------+--------+---------+
```

* **Magic header:** `NYXPRJ` (6 bytes).
* **Version:** `u16` little-endian. Current format: **v1**.
* **Payload:** [zstd](https://facebook.github.io/zstd/)-compressed UTF-8 JSON at compression level 3. The decompressed JSON is human-readable and can be inspected with:

```bash
tail -c +9 my-engagement.nyxproxy | zstd -d | jq .
```

## What's saved

```json
{
  "name": "Acme bug bounty 2025-Q1",
  "notes": "Found weird /api/internal/* endpoint…",
  "scope": ["*.acme.test", "api.acme.test"],
  "history": [/* every captured HttpFlow */],
  "issues": [/* every scanner finding */],
  "saved_at": "2025-05-13T20:00:00Z",
  "app_version": "0.1.0"
}
```

## Saving / loading from the desktop app

Open **Project options → Workspaces**:

1. **Save workspace…** — pick a destination path (native file dialog or browser prompt). The current scope, history, and issues are captured immediately and written.
2. **Open workspace…** — pick a `.nyxproxy` file; the workspace metadata is shown in a banner. Loading does not currently merge the history/issues into the live session — that ships in PR #5.

## Where the implementation lives

* Format + (de)serialisation: <ref_file file="/home/ubuntu/nyxproxy/apps/desktop/crates/nyxproxy-core/src/workspace.rs" /> — 4 unit tests in the same file.
* Tauri commands: `workspace_save_cmd`, `workspace_load_cmd`.
* React UI: `src/pages/ProjectOptions.tsx` (`WorkspacePanel`).
* TypeScript wrapper: `WorkspaceApi` in `src/tauri/api.ts`.
