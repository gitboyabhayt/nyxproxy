import { useEffect, useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Film,
  ListChecks,
  Play,
  Plus,
  Save,
  Trash2,
  Upload,
} from "lucide-react";

import { useAppStore } from "@/state/store";
import { MacrosApi, PlaywrightApi } from "@/tauri/api";
import type {
  Extraction,
  ExtractionSource,
  Macro,
  MacroRunResult,
  MacroStep,
  PlaywrightAction,
  PlaywrightAvailability,
  PlaywrightRecording,
} from "@/tauri/types";

const SOURCES: { value: ExtractionSource; label: string; hint: string }[] = [
  { value: "header", label: "Header", hint: "e.g. Location" },
  { value: "cookie", label: "Cookie", hint: "e.g. session" },
  { value: "json_pointer", label: "JSON pointer", hint: "/data/token" },
  { value: "body_regex", label: "Body regex (group 1)", hint: 'name="csrf" value="([^"]+)"' },
];

function emptyStep(): MacroStep {
  return {
    id: crypto.randomUUID(),
    name: "step",
    request: {
      method: "GET",
      url: "https://example.com/",
      headers: [],
      body_b64: "",
      follow_redirects: true,
      insecure: false,
    },
    extractions: [],
  };
}

function emptyMacro(): Macro {
  const now = new Date().toISOString();
  return {
    id: crypto.randomUUID(),
    name: "New macro",
    description: "",
    steps: [emptyStep()],
    created_at: now,
    updated_at: now,
  };
}

function encodeBody(text: string): string {
  return btoa(
    encodeURIComponent(text).replace(/%([0-9A-F]{2})/g, (_, h) =>
      String.fromCharCode(parseInt(h, 16)),
    ),
  );
}

function decodeBody(b64: string): string {
  if (!b64) return "";
  try {
    return decodeURIComponent(
      atob(b64)
        .split("")
        .map((c) => "%" + c.charCodeAt(0).toString(16).padStart(2, "0"))
        .join(""),
    );
  } catch {
    return atob(b64);
  }
}

function PlaywrightRecordingsSection() {
  const toast = useAppStore((s) => s.toast);
  const [open, setOpen] = useState(false);
  const [availability, setAvailability] = useState<PlaywrightAvailability | null>(
    null,
  );
  const [recordings, setRecordings] = useState<PlaywrightRecording[]>([]);
  const [importOpen, setImportOpen] = useState(false);
  const [importName, setImportName] = useState("");
  const [importDescription, setImportDescription] = useState("");
  const [importSpec, setImportSpec] = useState("");
  const [selected, setSelected] = useState<PlaywrightRecording | null>(null);

  const loadAvailability = async () => {
    try {
      setAvailability(await PlaywrightApi.detect());
    } catch (err) {
      toast("error", `Detect Playwright failed: ${err}`);
    }
  };

  const loadRecordings = async () => {
    try {
      setRecordings(await PlaywrightApi.list());
    } catch (err) {
      toast("error", `Recording list failed: ${err}`);
    }
  };

  useEffect(() => {
    if (!open) return;
    loadAvailability();
    loadRecordings();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const importSpecSubmit = async () => {
    if (!importSpec.trim()) {
      toast("error", "Paste a Playwright .spec.ts before importing.");
      return;
    }
    try {
      const saved = await PlaywrightApi.importSpec(
        importName.trim() || "Recorded macro",
        importSpec,
        importDescription,
      );
      toast(
        "info",
        `Imported "${saved.name}" with ${saved.actions.length} action(s).`,
      );
      setImportSpec("");
      setImportName("");
      setImportDescription("");
      setImportOpen(false);
      await loadRecordings();
    } catch (err) {
      toast("error", `Import failed: ${err}`);
    }
  };

  const remove = async (id: string) => {
    if (!confirm("Delete this recording?")) return;
    try {
      await PlaywrightApi.delete(id);
      if (selected?.id === id) setSelected(null);
      await loadRecordings();
    } catch (err) {
      toast("error", `Delete failed: ${err}`);
    }
  };

  return (
    <div className="panel" style={{ marginBottom: 8 }}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="panel-header"
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          width: "100%",
          background: "transparent",
          border: "none",
          textAlign: "left",
          cursor: "pointer",
          padding: "8px 12px",
          color: "inherit",
        }}
      >
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        <Film size={14} />
        <strong>Browser-recorded macros (Playwright)</strong>
        <span style={{ fontSize: 11, color: "var(--text-dim)", marginLeft: 8 }}>
          {recordings.length} saved
        </span>
      </button>
      {open && (
        <div className="panel-body" style={{ padding: 12, display: "flex", flexDirection: "column", gap: 10 }}>
          {availability && (
            <div
              className="row-wrap"
              style={{ alignItems: "center", fontSize: 12 }}
            >
              <span
                className={`badge ${availability.available ? "badge-low" : "badge-medium"}`}
              >
                {availability.available
                  ? `Playwright ${availability.version ?? ""}`
                  : "Playwright not detected"}
              </span>
              <span className="muted">{availability.install_hint}</span>
            </div>
          )}
          <div className="row-wrap">
            <button
              className="btn primary"
              type="button"
              onClick={() => setImportOpen((v) => !v)}
            >
              <Upload size={14} /> Import codegen .spec.ts
            </button>
            <span style={{ fontSize: 11, color: "var(--text-dim)" }}>
              Record with{" "}
              <code className="kbd">
                npx playwright codegen --target javascript &lt;url&gt;
              </code>
              {" "}then paste the output here.
            </span>
          </div>
          {importOpen && (
            <div className="panel" style={{ padding: 10 }}>
              <div className="row-wrap" style={{ marginBottom: 8 }}>
                <input
                  placeholder="Recording name"
                  value={importName}
                  onChange={(e) => setImportName(e.target.value)}
                  style={{ flex: 1, minWidth: 200 }}
                />
              </div>
              <input
                placeholder="Description (optional)"
                value={importDescription}
                onChange={(e) => setImportDescription(e.target.value)}
                style={{ width: "100%", marginBottom: 8 }}
              />
              <textarea
                className="mono"
                rows={10}
                value={importSpec}
                onChange={(e) => setImportSpec(e.target.value)}
                placeholder={`import { test, expect } from '@playwright/test';\n\ntest('login', async ({ page }) => {\n  await page.goto('https://example.com/login');\n  await page.getByRole('textbox', { name: 'Email' }).fill('user@example.com');\n  // ...\n});`}
                style={{ width: "100%" }}
              />
              <div className="row-wrap" style={{ marginTop: 8 }}>
                <button className="btn primary" type="button" onClick={importSpecSubmit}>
                  Import
                </button>
                <button
                  className="btn ghost"
                  type="button"
                  onClick={() => setImportOpen(false)}
                >
                  Cancel
                </button>
              </div>
            </div>
          )}
          {recordings.length === 0 ? (
            <div className="muted" style={{ fontSize: 12 }}>
              No recordings yet — import a codegen spec to capture a login flow.
            </div>
          ) : (
            <div className="row-wrap" style={{ alignItems: "flex-start" }}>
              <div
                className="panel"
                style={{ flex: "1 1 220px", minWidth: 220, maxHeight: 260, overflow: "auto" }}
              >
                {recordings.map((r) => (
                  <div
                    key={r.id}
                    className={`nav-item ${selected?.id === r.id ? "active" : ""}`}
                    onClick={() => setSelected(r)}
                    style={{ flexDirection: "column", alignItems: "flex-start", gap: 2 }}
                  >
                    <span style={{ fontWeight: 600 }}>{r.name}</span>
                    <span style={{ fontSize: 11, color: "var(--text-dim)" }}>
                      {r.actions.length} action{r.actions.length === 1 ? "" : "s"}
                    </span>
                  </div>
                ))}
              </div>
              <div
                className="panel"
                style={{
                  flex: "2 1 320px",
                  minWidth: 280,
                  padding: 10,
                  maxHeight: 260,
                  overflow: "auto",
                }}
              >
                {!selected ? (
                  <div className="muted" style={{ fontSize: 12 }}>
                    Select a recording on the left to inspect its steps.
                  </div>
                ) : (
                  <>
                    <div className="row-wrap" style={{ marginBottom: 8 }}>
                      <strong>{selected.name}</strong>
                      <span className="muted" style={{ fontSize: 11 }}>
                        Saved {new Date(selected.updated_at).toLocaleString()}
                      </span>
                      <span style={{ flex: 1 }} />
                      <button
                        className="btn danger small"
                        type="button"
                        onClick={() => remove(selected.id)}
                      >
                        <Trash2 size={12} /> Delete
                      </button>
                    </div>
                    {selected.description && (
                      <p className="muted" style={{ fontSize: 12 }}>
                        {selected.description}
                      </p>
                    )}
                    <ol
                      style={{
                        margin: 0,
                        paddingLeft: 18,
                        fontSize: 12,
                        fontFamily: "var(--font-mono)",
                      }}
                    >
                      {selected.actions.map((a, i) => (
                        <li key={i} style={{ marginBottom: 4 }}>
                          {renderPlaywrightAction(a)}
                        </li>
                      ))}
                    </ol>
                  </>
                )}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function renderPlaywrightAction(a: PlaywrightAction): string {
  switch (a.kind) {
    case "navigate":
      return `navigate("${a.url}")`;
    case "click":
      return `click(${a.selector})`;
    case "fill":
      return `fill(${a.selector}, "${a.value}")`;
    case "press":
      return `press(${a.selector}, "${a.key}")`;
    case "wait_for_url":
      return `wait_for_url("${a.url}")`;
    case "expect_url":
      return `expect_url("${a.url}")`;
    case "raw":
      return `// ${a.line}`;
  }
}

export function MacrosPage() {
  const toast = useAppStore((s) => s.toast);
  const [macros, setMacros] = useState<Macro[]>([]);
  const [draft, setDraft] = useState<Macro | null>(null);
  const [lastRun, setLastRun] = useState<MacroRunResult | null>(null);
  const [running, setRunning] = useState(false);

  const refresh = async () => {
    try {
      setMacros(await MacrosApi.list());
    } catch (err) {
      toast("error", `Macro list failed: ${err}`);
    }
  };

  useEffect(() => {
    refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selectMacro = (id: string) => {
    setDraft(macros.find((m) => m.id === id) ?? null);
    setLastRun(null);
  };

  const newMacro = () => {
    const m = emptyMacro();
    setDraft(m);
    setLastRun(null);
  };

  const save = async () => {
    if (!draft) return;
    try {
      const saved = await MacrosApi.save(draft);
      await refresh();
      setDraft(saved);
      toast("info", `Saved macro "${saved.name}".`);
    } catch (err) {
      toast("error", `Save failed: ${err}`);
    }
  };

  const remove = async () => {
    if (!draft) return;
    if (!confirm(`Delete macro "${draft.name}"?`)) return;
    try {
      await MacrosApi.delete(draft.id);
      await refresh();
      setDraft(null);
      setLastRun(null);
    } catch (err) {
      toast("error", `Delete failed: ${err}`);
    }
  };

  const run = async () => {
    if (!draft) return;
    setRunning(true);
    try {
      const result = await MacrosApi.run(draft.id, {});
      setLastRun(result);
      const ok = result.succeeded;
      toast(
        ok ? "info" : "error",
        `Macro ${ok ? "ran" : "failed"}: ${result.steps.length} step(s), ${
          Object.keys(result.final_variables).length
        } variable(s).`,
      );
    } catch (err) {
      toast("error", `Run failed: ${err}`);
    } finally {
      setRunning(false);
    }
  };

  const update = (patch: Partial<Macro>) => {
    if (!draft) return;
    setDraft({ ...draft, ...patch });
  };

  const updateStep = (idx: number, patch: Partial<MacroStep>) => {
    if (!draft) return;
    const steps = [...draft.steps];
    const current = steps[idx];
    if (!current) return;
    steps[idx] = { ...current, ...patch };
    update({ steps });
  };

  const updateRequest = (idx: number, patch: Partial<MacroStep["request"]>) => {
    if (!draft) return;
    const step = draft.steps[idx];
    if (!step) return;
    updateStep(idx, { request: { ...step.request, ...patch } });
  };

  const addStep = () => {
    if (!draft) return;
    update({ steps: [...draft.steps, emptyStep()] });
  };

  const removeStep = (idx: number) => {
    if (!draft) return;
    update({ steps: draft.steps.filter((_, i) => i !== idx) });
  };

  const moveStep = (idx: number, dir: -1 | 1) => {
    if (!draft) return;
    const target = idx + dir;
    if (target < 0 || target >= draft.steps.length) return;
    const steps = [...draft.steps];
    const a = steps[idx];
    const b = steps[target];
    if (!a || !b) return;
    steps[idx] = b;
    steps[target] = a;
    update({ steps });
  };

  const addExtraction = (idx: number) => {
    if (!draft) return;
    const step = draft.steps[idx];
    if (!step) return;
    const extractions = [
      ...step.extractions,
      { name: "var", source: "header" as ExtractionSource, pattern: "" } as Extraction,
    ];
    updateStep(idx, { extractions });
  };

  const updateExtraction = (
    stepIdx: number,
    extIdx: number,
    patch: Partial<Extraction>,
  ) => {
    if (!draft) return;
    const step = draft.steps[stepIdx];
    if (!step) return;
    const extractions = [...step.extractions];
    const current = extractions[extIdx];
    if (!current) return;
    extractions[extIdx] = { ...current, ...patch };
    updateStep(stepIdx, { extractions });
  };

  const removeExtraction = (stepIdx: number, extIdx: number) => {
    if (!draft) return;
    const step = draft.steps[stepIdx];
    if (!step) return;
    const extractions = step.extractions.filter((_, i) => i !== extIdx);
    updateStep(stepIdx, { extractions });
  };

  const headerText = (idx: number): string => {
    const step = draft?.steps[idx];
    if (!step) return "";
    return step.request.headers.map((h) => `${h.name}: ${h.value}`).join("\n");
  };

  const setHeaderText = (idx: number, text: string) => {
    const headers = text
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        const colon = line.indexOf(":");
        if (colon === -1) return { name: line, value: "" };
        return {
          name: line.slice(0, colon).trim(),
          value: line.slice(colon + 1).trim(),
        };
      });
    updateRequest(idx, { headers });
  };

  const list = useMemo(() => macros, [macros]);

  return (
    <>
      <div className="toolbar" style={{ gap: 8 }}>
        <button className="btn primary" onClick={newMacro}>
          <Plus size={14} /> New macro
        </button>
        {draft && (
          <>
            <button className="btn ghost" onClick={save}>
              <Save size={14} /> Save
            </button>
            <button
              className="btn primary"
              onClick={run}
              disabled={running || draft.steps.length === 0}
            >
              <Play size={14} /> {running ? "Running…" : "Run"}
            </button>
            <button className="btn danger" onClick={remove}>
              <Trash2 size={14} /> Delete
            </button>
          </>
        )}
      </div>

      <PlaywrightRecordingsSection />

      <div
        className="main-content"
        style={{ display: "flex", gap: 12, alignItems: "stretch", overflow: "hidden" }}
      >
        <div className="panel" style={{ flexBasis: 240, flexShrink: 0 }}>
          <div className="panel-header">Macros ({list.length})</div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            {list.length === 0 ? (
              <div className="empty-state">
                <p>No macros yet — click <strong>New macro</strong>.</p>
              </div>
            ) : (
              list.map((m) => (
                <div
                  key={m.id}
                  className={`nav-item ${draft?.id === m.id ? "active" : ""}`}
                  onClick={() => selectMacro(m.id)}
                  style={{ alignItems: "flex-start", flexDirection: "column", gap: 2 }}
                >
                  <span style={{ fontWeight: 600 }}>{m.name}</span>
                  <span style={{ fontSize: 11, color: "var(--text-dim)" }}>
                    {m.steps.length} step{m.steps.length === 1 ? "" : "s"}
                  </span>
                </div>
              ))
            )}
          </div>
        </div>

        <div className="panel" style={{ flex: 1, overflow: "auto" }}>
          {!draft ? (
            <div className="empty-state">
              <h3>Macros</h3>
              <p>
                A macro is a sequence of requests played back in order. Each step's response can
                populate variables (header, cookie, JSON pointer, body regex) that are
                interpolated into subsequent step's URL, headers, or body via{" "}
                <code className="mono">{"{{name}}"}</code> syntax. Use this for login flows,
                CSRF token refresh, or any pre-request chain.
              </p>
            </div>
          ) : (
            <div style={{ padding: 12, display: "flex", flexDirection: "column", gap: 14 }}>
              <div className="row">
                <div className="field" style={{ flex: 1 }}>
                  <label className="label">Name</label>
                  <input value={draft.name} onChange={(e) => update({ name: e.target.value })} />
                </div>
              </div>
              <div className="field">
                <label className="label">Description</label>
                <textarea
                  rows={2}
                  value={draft.description}
                  onChange={(e) => update({ description: e.target.value })}
                />
              </div>

              {draft.steps.map((step, idx) => (
                <div className="panel" key={step.id}>
                  <div className="panel-header" style={{ display: "flex", gap: 8 }}>
                    <ListChecks size={14} />
                    <strong style={{ flex: 1 }}>
                      Step {idx + 1}: {step.name || "(unnamed)"}
                    </strong>
                    <button className="btn small ghost" onClick={() => moveStep(idx, -1)}>
                      ↑
                    </button>
                    <button className="btn small ghost" onClick={() => moveStep(idx, 1)}>
                      ↓
                    </button>
                    <button className="btn small danger" onClick={() => removeStep(idx)}>
                      Remove
                    </button>
                  </div>
                  <div
                    className="panel-body"
                    style={{ padding: 12, display: "flex", flexDirection: "column", gap: 8 }}
                  >
                    <div className="row">
                      <div className="field">
                        <label className="label">Name</label>
                        <input
                          value={step.name}
                          onChange={(e) => updateStep(idx, { name: e.target.value })}
                        />
                      </div>
                      <div className="field" style={{ width: 110 }}>
                        <label className="label">Method</label>
                        <select
                          value={step.request.method}
                          onChange={(e) => updateRequest(idx, { method: e.target.value })}
                        >
                          {["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"].map((m) => (
                            <option key={m} value={m}>
                              {m}
                            </option>
                          ))}
                        </select>
                      </div>
                      <div className="field" style={{ flex: 1 }}>
                        <label className="label">URL (supports {"{{var}}"})</label>
                        <input
                          className="mono"
                          value={step.request.url}
                          onChange={(e) => updateRequest(idx, { url: e.target.value })}
                        />
                      </div>
                    </div>
                    <div className="field">
                      <label className="label">Headers — one per line, "Name: value"</label>
                      <textarea
                        className="mono"
                        rows={3}
                        value={headerText(idx)}
                        onChange={(e) => setHeaderText(idx, e.target.value)}
                      />
                    </div>
                    <div className="field">
                      <label className="label">Body</label>
                      <textarea
                        className="mono"
                        rows={4}
                        value={decodeBody(step.request.body_b64)}
                        onChange={(e) =>
                          updateRequest(idx, { body_b64: encodeBody(e.target.value) })
                        }
                      />
                    </div>

                    <div>
                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: 8,
                          marginBottom: 6,
                        }}
                      >
                        <strong>Extractions</strong>
                        <button className="btn small ghost" onClick={() => addExtraction(idx)}>
                          + Add
                        </button>
                      </div>
                      {step.extractions.length === 0 ? (
                        <div style={{ color: "var(--text-dim)", fontSize: 12 }}>
                          (No extractions — this step won't set any variables.)
                        </div>
                      ) : (
                        step.extractions.map((ext, ei) => (
                          <div key={ei} className="row" style={{ marginTop: 4 }}>
                            <div className="field" style={{ width: 140 }}>
                              <label className="label">Variable</label>
                              <input
                                className="mono"
                                value={ext.name}
                                onChange={(e) =>
                                  updateExtraction(idx, ei, { name: e.target.value })
                                }
                              />
                            </div>
                            <div className="field" style={{ width: 180 }}>
                              <label className="label">Source</label>
                              <select
                                value={ext.source}
                                onChange={(e) =>
                                  updateExtraction(idx, ei, {
                                    source: e.target.value as ExtractionSource,
                                  })
                                }
                              >
                                {SOURCES.map((s) => (
                                  <option key={s.value} value={s.value}>
                                    {s.label}
                                  </option>
                                ))}
                              </select>
                            </div>
                            <div className="field" style={{ flex: 1 }}>
                              <label className="label">
                                Pattern (
                                {SOURCES.find((s) => s.value === ext.source)?.hint})
                              </label>
                              <input
                                className="mono"
                                value={ext.pattern}
                                onChange={(e) =>
                                  updateExtraction(idx, ei, { pattern: e.target.value })
                                }
                              />
                            </div>
                            <button
                              className="btn small danger"
                              onClick={() => removeExtraction(idx, ei)}
                              style={{ alignSelf: "end" }}
                            >
                              ×
                            </button>
                          </div>
                        ))
                      )}
                    </div>
                  </div>
                </div>
              ))}

              <button className="btn ghost" onClick={addStep}>
                <Plus size={14} /> Add step
              </button>

              {lastRun && (
                <div className="panel">
                  <div className="panel-header">
                    Last run · {lastRun.succeeded ? "OK" : "Failed"}
                  </div>
                  <div
                    className="panel-body"
                    style={{ padding: 12, display: "flex", flexDirection: "column", gap: 8 }}
                  >
                    <div>
                      <strong>Final variables</strong>
                      <pre className="code" style={{ maxHeight: 160, overflow: "auto" }}>
                        {Object.keys(lastRun.final_variables).length
                          ? Object.entries(lastRun.final_variables)
                              .map(([k, v]) => `${k} = ${v}`)
                              .join("\n")
                          : "(none)"}
                      </pre>
                    </div>
                    <div>
                      <strong>Steps</strong>
                      <table className="data-table">
                        <thead>
                          <tr>
                            <th>#</th>
                            <th>Name</th>
                            <th>Status</th>
                            <th>Duration</th>
                            <th>Extracted</th>
                            <th>Error</th>
                          </tr>
                        </thead>
                        <tbody>
                          {lastRun.steps.map((s, i) => (
                            <tr key={i}>
                              <td>{i + 1}</td>
                              <td>{s.step_name}</td>
                              <td>{s.response?.status ?? "—"}</td>
                              <td>{s.duration_ms} ms</td>
                              <td className="mono">
                                {Object.keys(s.extracted).length
                                  ? Object.entries(s.extracted)
                                      .map(([k, v]) => `${k}=${v}`)
                                      .join(", ")
                                  : "—"}
                              </td>
                              <td style={{ color: "var(--text-danger)" }}>{s.error ?? "—"}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </>
  );
}
