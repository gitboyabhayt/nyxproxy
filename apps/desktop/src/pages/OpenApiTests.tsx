import { useMemo, useState } from "react";

import {
  OpenApiApi,
  type OpenApiPlan,
  type OpenApiTestCase,
} from "@/tauri/api";
import { useAppStore } from "@/state/store";

export function OpenApiTestsPage() {
  const toast = useAppStore((s) => s.toast);
  const [path, setPath] = useState<string>("");
  const [baseOverride, setBaseOverride] = useState<string>("");
  const [plan, setPlan] = useState<OpenApiPlan | null>(null);
  const [busy, setBusy] = useState(false);

  const counts = useMemo(() => {
    if (!plan) return { auth: 0, idor: 0, rate: 0 };
    return {
      auth: plan.cases.filter((c) => c.category === "auth-bypass").length,
      idor: plan.cases.filter((c) => c.category === "idor").length,
      rate: plan.cases.filter((c) => c.category === "rate-limit").length,
    };
  }, [plan]);

  async function pickSpecPath(): Promise<string | null> {
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const chosen = await dialog.open({
        title: "Choose OpenAPI / Swagger JSON",
        multiple: false,
        filters: [
          { name: "OpenAPI / Swagger JSON", extensions: ["json"] },
          { name: "All files", extensions: ["*"] },
        ],
      });
      if (!chosen) return null;
      return typeof chosen === "string" ? chosen : null;
    } catch {
      const fallback = window.prompt("Enter path to swagger.json:", path);
      return fallback && fallback.trim() ? fallback.trim() : null;
    }
  }

  const onChooseFile = async () => {
    const chosen = await pickSpecPath();
    if (chosen) setPath(chosen);
  };

  const onBuildPlan = async () => {
    if (!path.trim()) {
      toast("warning", "Pick a swagger.json first");
      return;
    }
    setBusy(true);
    try {
      const built = await OpenApiApi.buildPlan(
        path.trim(),
        baseOverride.trim() ? baseOverride.trim() : undefined,
      );
      setPlan(built);
      if (built.cases.length === 0) {
        toast("warning", `No test cases generated: ${built.diagnostics.join("; ")}`);
      } else {
        toast(
          "info",
          `Generated ${built.cases.length} cases against ${built.server_url}`,
        );
      }
    } catch (err) {
      toast("error", `Plan failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="section" style={{ overflow: "auto" }}>
      <div>
        <h2>OpenAPI auto-tests</h2>
        <p style={{ color: "var(--text-dim)" }}>
          Drop a <code>swagger.json</code> or <code>openapi.json</code> to
          generate three classes of test requests automatically:
          auth-bypass, IDOR (numeric id mutation), and rate-limit /
          brute-force on auth-shaped endpoints. The plan is read-only —
          send any case to Repeater or Intruder when you're ready to fire.
        </p>
      </div>

      <div className="panel">
        <div className="panel-header">Spec source</div>
        <div className="panel-body" style={{ padding: 12, gap: 8 }}>
          <div className="field">
            <label className="label">OpenAPI / Swagger JSON path</label>
            <div style={{ display: "flex", gap: 8 }}>
              <input
                style={{ flex: 1, fontFamily: "monospace" }}
                value={path}
                onChange={(e) => setPath(e.target.value)}
                placeholder="/path/to/swagger.json"
                spellCheck={false}
              />
              <button className="btn" disabled={busy} onClick={onChooseFile}>
                Choose file…
              </button>
            </div>
          </div>
          <div className="field">
            <label className="label">
              Base override (optional — overrides <code>servers[0].url</code>)
            </label>
            <input
              value={baseOverride}
              onChange={(e) => setBaseOverride(e.target.value)}
              placeholder="https://staging.example.com/v1"
              spellCheck={false}
            />
          </div>
          <div>
            <button className="btn primary" disabled={busy} onClick={onBuildPlan}>
              Build test plan
            </button>
          </div>
        </div>
      </div>

      {plan && (
        <div className="panel">
          <div className="panel-header">
            Plan — {plan.cases.length} cases against{" "}
            <code>{plan.server_url}</code> (spec {plan.version})
          </div>
          <div className="panel-body" style={{ padding: 12, gap: 10 }}>
            <div style={{ display: "flex", gap: 16, flexWrap: "wrap" }}>
              <span>
                <b>{counts.auth}</b> auth-bypass
              </span>
              <span>
                <b>{counts.idor}</b> IDOR
              </span>
              <span>
                <b>{counts.rate}</b> rate-limit
              </span>
            </div>
            {plan.diagnostics.length > 0 && (
              <div className="banner info">
                Diagnostics:{" "}
                <code>{plan.diagnostics.join(" · ")}</code>
              </div>
            )}
            <PlanTable cases={plan.cases} />
          </div>
        </div>
      )}
    </div>
  );
}

function PlanTable({ cases }: { cases: OpenApiTestCase[] }) {
  if (cases.length === 0) return null;
  return (
    <table
      style={{
        width: "100%",
        borderCollapse: "collapse",
        fontSize: 13,
        tableLayout: "fixed",
      }}
    >
      <thead>
        <tr style={{ background: "var(--panel)" }}>
          <th style={th("80px")}>Category</th>
          <th style={th("60px")}>Method</th>
          <th style={th(null)}>URL</th>
          <th style={th("80px")}>Repeat</th>
          <th style={th(null)}>Notes</th>
        </tr>
      </thead>
      <tbody>
        {cases.map((c, i) => (
          <tr key={i} style={{ borderTop: "1px solid var(--panel-border)" }}>
            <td style={td()}>
              <span className={`tag tag-${c.category}`}>{c.category}</span>
            </td>
            <td style={td()}>
              <code>{c.method}</code>
            </td>
            <td style={td()}>
              <code style={{ wordBreak: "break-all" }}>{c.url}</code>
            </td>
            <td style={td()}>{c.repeat}</td>
            <td style={td()}>{c.notes}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function th(width: string | null): React.CSSProperties {
  return {
    textAlign: "left",
    padding: "8px 6px",
    fontWeight: 600,
    ...(width ? { width } : {}),
  };
}

function td(): React.CSSProperties {
  return { padding: "8px 6px", verticalAlign: "top" };
}
