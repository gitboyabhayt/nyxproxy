import { useEffect, useMemo, useState } from "react";
import { ListChecks, Play, Plus, Save, Trash2 } from "lucide-react";

import { useAppStore } from "@/state/store";
import { MacrosApi } from "@/tauri/api";
import type {
  Extraction,
  ExtractionSource,
  Macro,
  MacroRunResult,
  MacroStep,
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
