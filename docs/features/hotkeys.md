# Command palette (Ctrl + K)

NyxProxy ships with a global command palette modelled on VS Code, Linear and Raycast. It lets you jump to any page or fire any action without leaving the keyboard.

## Activation

| Platform | Shortcut |
|---|---|
| Linux / Windows | `Ctrl + K` |
| macOS | `⌘ + K` |

Press the same shortcut to toggle the palette, or `Esc` to dismiss.

## What's inside

* **Navigation** — *Go to Dashboard*, *Go to Proxy*, *Go to Repeater*, … one entry per page (tools + options).
* **Actions** — *Start proxy* / *Stop proxy*, *Open sidebar* / *Close sidebar*.

The palette uses a forgiving matcher:

* exact substring → top hit
* word-prefix → boosted
* characters-in-order (fuzzy) → still matches

Use `↑` / `↓` to move and `Enter` to run. Recently-used commands float to the top when the search is empty; storage is in `localStorage` under `nyxproxy:cmd-palette:recent`.

## Adding new commands

Open <ref_file file="/home/ubuntu/nyxproxy/apps/desktop/src/App.tsx" /> and append to `paletteCommands` — each entry is a `{ id, label, group, run }` object.

## Where the implementation lives

* Component: `src/components/CommandPalette.tsx`
* Wiring: `src/App.tsx` (keyboard listener + command list)
* Styles: `.command-palette*` rules in `src/styles/globals.css`
