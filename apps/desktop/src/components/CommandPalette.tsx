import { useCallback, useEffect, useMemo, useRef, useState } from "react";

const RECENT_KEY = "nyxproxy:cmd-palette:recent";
const RECENT_LIMIT = 5;

export interface PaletteCommand {
  id: string;
  label: string;
  hint?: string;
  shortcut?: string;
  group?: string;
  keywords?: string[];
  run: () => void | Promise<void>;
}

interface Props {
  open: boolean;
  commands: PaletteCommand[];
  onClose: () => void;
}

function score(cmd: PaletteCommand, q: string): number {
  if (!q) return 1;
  const needle = q.toLowerCase();
  const haystack = [cmd.label, cmd.hint ?? "", cmd.group ?? "", ...(cmd.keywords ?? [])]
    .join(" ")
    .toLowerCase();
  if (haystack.includes(needle)) {
    let s = 100;
    if (cmd.label.toLowerCase().startsWith(needle)) s += 50;
    if (cmd.label.toLowerCase() === needle) s += 100;
    return s;
  }
  // letters-in-order fuzzy match
  let j = 0;
  for (const ch of haystack) {
    if (ch === needle[j]) j++;
    if (j === needle.length) return 1;
  }
  return 0;
}

function loadRecent(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as string[]).slice(0, RECENT_LIMIT) : [];
  } catch {
    return [];
  }
}

function pushRecent(id: string) {
  try {
    const cur = loadRecent().filter((x) => x !== id);
    cur.unshift(id);
    localStorage.setItem(RECENT_KEY, JSON.stringify(cur.slice(0, RECENT_LIMIT)));
  } catch {
    /* ignore */
  }
}

export function CommandPalette({ open, commands, onClose }: Props) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  const filtered = useMemo(() => {
    const recent = loadRecent();
    if (!query.trim()) {
      // No query: prefer recent first, then alphabetical
      const recents = recent
        .map((id) => commands.find((c) => c.id === id))
        .filter((c): c is PaletteCommand => !!c);
      const rest = commands
        .filter((c) => !recent.includes(c.id))
        .sort((a, b) => a.label.localeCompare(b.label));
      return [...recents, ...rest];
    }
    return commands
      .map((c) => ({ c, s: score(c, query) }))
      .filter((x) => x.s > 0)
      .sort((a, b) => b.s - a.s)
      .map((x) => x.c);
  }, [query, commands]);

  useEffect(() => {
    if (open) {
      setQuery("");
      setActive(0);
      const id = requestAnimationFrame(() => inputRef.current?.focus());
      return () => cancelAnimationFrame(id);
    }
    return undefined;
  }, [open]);

  useEffect(() => {
    if (active >= filtered.length) setActive(0);
  }, [filtered.length, active]);

  const select = useCallback(
    (cmd: PaletteCommand | undefined) => {
      if (!cmd) return;
      pushRecent(cmd.id);
      onClose();
      void cmd.run();
    },
    [onClose],
  );

  if (!open) return null;

  return (
    <div
      className="command-palette-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      onClick={onClose}
    >
      <div
        className="command-palette"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            onClose();
          } else if (e.key === "ArrowDown") {
            e.preventDefault();
            setActive((i) => Math.min(filtered.length - 1, i + 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive((i) => Math.max(0, i - 1));
          } else if (e.key === "Enter") {
            e.preventDefault();
            select(filtered[active]);
          }
        }}
      >
        <input
          ref={inputRef}
          className="command-palette-input"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setActive(0);
          }}
          placeholder="Type a command, page, or action…"
          aria-label="Command search"
        />
        <div ref={listRef} className="command-palette-list">
          {filtered.length === 0 ? (
            <div className="command-palette-empty">No matching commands</div>
          ) : (
            filtered.map((cmd, idx) => (
              <button
                key={cmd.id}
                type="button"
                className={`command-palette-item ${idx === active ? "active" : ""}`}
                onMouseEnter={() => setActive(idx)}
                onClick={() => select(cmd)}
              >
                <span className="command-palette-label">{cmd.label}</span>
                {cmd.group && <span className="command-palette-group">{cmd.group}</span>}
                {cmd.shortcut && (
                  <span className="command-palette-shortcut">{cmd.shortcut}</span>
                )}
              </button>
            ))
          )}
        </div>
        <div className="command-palette-foot">
          <span>↑↓ navigate</span>
          <span>↵ run</span>
          <span>Esc close</span>
        </div>
      </div>
    </div>
  );
}
