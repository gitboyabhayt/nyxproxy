import { useEffect, useMemo, useState } from "react";

import { useAppStore } from "@/state/store";
import { AiApi } from "@/tauri/api";
import type {
  AutoAttackPlan,
  ChainScanResponse,
  FuzzMutateResponse,
  HttpRequestPayload,
  HttpResponsePayload,
  VulnClass,
} from "@/tauri/types";

const ALL_VULNS: VulnClass[] = [
  "sqli",
  "xss",
  "ssrf",
  "lfi",
  "rce",
  "open_redirect",
  "ssti",
  "xxe",
  "auth_bypass",
  "idor",
  "csrf",
  "jwt",
  "deserialization",
  "graphql_injection",
  "nosql",
  "log4shell",
  "prototype_pollution",
  "race_condition",
];

const ATTACK_TYPES = ["xss", "sqli", "ssrf", "lfi", "rce", "open_redirect", "ssti"];

type Tab = "auto-attack" | "chain-scan" | "fuzz";

function flowToRequest(flow: { request: { method: string; url: string; http_version: string; headers: { name: string; value: string }[] } }): HttpRequestPayload {
  return {
    method: flow.request.method,
    url: flow.request.url,
    http_version: flow.request.http_version,
    headers: flow.request.headers.reduce<Record<string, string>>((acc, h) => {
      acc[h.name] = h.value;
      return acc;
    }, {}),
    body: null,
  };
}

function flowToResponse(flow: {
  response: {
    status: number;
    http_version: string;
    headers: { name: string; value: string }[];
  } | null;
}): HttpResponsePayload | null {
  if (!flow.response) return null;
  return {
    status: flow.response.status,
    http_version: flow.response.http_version,
    headers: flow.response.headers.reduce<Record<string, string>>((acc, h) => {
      acc[h.name] = h.value;
      return acc;
    }, {}),
    body: null,
  };
}

export function AiAttackPage() {
  const providers = useAppStore((s) => s.providers);
  const reload = useAppStore((s) => s.reloadProviders);
  const toast = useAppStore((s) => s.toast);
  const history = useAppStore((s) => s.history);

  const [tab, setTab] = useState<Tab>("auto-attack");
  const [provider, setProvider] = useState<string>("");
  const [busy, setBusy] = useState(false);

  // Auto-attack state
  const [suspected, setSuspected] = useState<VulnClass[]>([]);
  const [payloadsPerClass, setPayloadsPerClass] = useState(5);
  const [plan, setPlan] = useState<AutoAttackPlan | null>(null);

  // Chain-scan state
  const [chain, setChain] = useState<ChainScanResponse | null>(null);

  // Fuzz state
  const [seed, setSeed] = useState("<script>alert(1)</script>");
  const [attackType, setAttackType] = useState("xss");
  const [count, setCount] = useState(8);
  const [fuzz, setFuzz] = useState<FuzzMutateResponse | null>(null);

  useEffect(() => {
    if (!providers) reload();
  }, [providers, reload]);

  const activeProvider =
    provider || providers?.default || providers?.providers.find((p) => p.available)?.name || "";

  const latestFlow = useMemo(() => history[0]?.flow ?? null, [history]);

  const toggleSuspected = (v: VulnClass) =>
    setSuspected((prev) => (prev.includes(v) ? prev.filter((x) => x !== v) : [...prev, v]));

  const runAutoAttack = async () => {
    if (!latestFlow) {
      toast("warning", "No captured flow yet — proxy something first.");
      return;
    }
    setBusy(true);
    setPlan(null);
    try {
      const resp = await AiApi.autoAttack({
        request: flowToRequest(latestFlow),
        response: flowToResponse(latestFlow),
        suspected: suspected.length ? suspected : undefined,
        payloads_per_class: payloadsPerClass,
        provider: activeProvider || null,
      });
      setPlan(resp);
      toast(
        "info",
        `Plan ready · ${resp.vectors.length} vectors via ${resp.provider}${resp.fallbacks_tried.length ? ` (fallback from ${resp.fallbacks_tried.join(", ")})` : ""}`,
      );
    } catch (err) {
      toast("error", `Auto-attack failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const runChainScan = async () => {
    if (!latestFlow) {
      toast("warning", "No captured flow yet — proxy something first.");
      return;
    }
    setBusy(true);
    setChain(null);
    try {
      const resp = await AiApi.chainScan({
        request: flowToRequest(latestFlow),
        response: flowToResponse(latestFlow),
        issues_seen: [],
        provider: activeProvider || null,
      });
      setChain(resp);
      toast("info", `Chain scan done · risk ${resp.risk_score} via ${resp.provider}`);
    } catch (err) {
      toast("error", `Chain scan failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const runFuzz = async () => {
    if (!seed.trim()) {
      toast("warning", "Provide a seed payload first.");
      return;
    }
    setBusy(true);
    setFuzz(null);
    try {
      const resp = await AiApi.fuzzMutate({
        seed,
        attack_type: attackType,
        count,
        provider: activeProvider || null,
      });
      setFuzz(resp);
      toast(
        "info",
        `${resp.mutations.length} mutations via ${resp.provider}${resp.fallbacks_tried.length ? ` (fallback)` : ""}`,
      );
    } catch (err) {
      toast("error", `Fuzz mutate failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="toolbar">
        <select value={activeProvider} onChange={(e) => setProvider(e.target.value)}>
          {providers?.providers.map((p) => (
            <option key={p.name} value={p.name} disabled={!p.available}>
              {p.name} {p.available ? "" : "(disabled)"}
            </option>
          ))}
        </select>
        <button className="btn" onClick={() => reload()}>
          Refresh
        </button>
        <span className="grow" />
        <div className="sub-tabs">
          <button
            className={`sub-tab ${tab === "auto-attack" ? "active" : ""}`}
            onClick={() => setTab("auto-attack")}
          >
            Auto-attack
          </button>
          <button
            className={`sub-tab ${tab === "chain-scan" ? "active" : ""}`}
            onClick={() => setTab("chain-scan")}
          >
            Chain scan
          </button>
          <button
            className={`sub-tab ${tab === "fuzz" ? "active" : ""}`}
            onClick={() => setTab("fuzz")}
          >
            Fuzz mutator
          </button>
        </div>
      </div>

      <div className="main-content" style={{ overflow: "auto", padding: 16 }}>
        {!latestFlow && tab !== "fuzz" && (
          <div className="empty-state">
            <h3>Capture a flow first</h3>
            <p>Send any request through the proxy, then come back here to launch AI-driven attacks.</p>
          </div>
        )}

        {tab === "auto-attack" && (
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <div className="card" style={{ padding: 12 }}>
              <h4 style={{ marginTop: 0 }}>Auto-attack plan</h4>
              <p style={{ color: "var(--text-2)", marginTop: 0 }}>
                Target:{" "}
                <code className="kbd">
                  {latestFlow ? `${latestFlow.request.method} ${latestFlow.request.url}` : "(none)"}
                </code>
              </p>
              <div style={{ display: "flex", flexWrap: "wrap", gap: 6, marginBottom: 8 }}>
                {ALL_VULNS.map((v) => (
                  <button
                    key={v}
                    className={`chip ${suspected.includes(v) ? "chip-active" : ""}`}
                    onClick={() => toggleSuspected(v)}
                  >
                    {v}
                  </button>
                ))}
              </div>
              <label className="row">
                Payloads / class
                <input
                  type="number"
                  min={1}
                  max={20}
                  value={payloadsPerClass}
                  onChange={(e) => setPayloadsPerClass(Math.max(1, Math.min(20, Number(e.target.value) || 1)))}
                />
              </label>
              <button className="btn btn-primary" onClick={runAutoAttack} disabled={busy || !latestFlow}>
                {busy ? "Running…" : "Generate plan"}
              </button>
            </div>

            {plan && (
              <div className="card" style={{ padding: 12 }}>
                <h4 style={{ marginTop: 0 }}>Summary</h4>
                <p>{plan.summary}</p>
                <p style={{ color: "var(--text-2)", fontSize: 12 }}>
                  via <strong>{plan.provider}</strong> ({plan.model})
                  {plan.fallbacks_tried.length > 0 &&
                    ` · fallbacks: ${plan.fallbacks_tried.join(", ")}`}
                </p>
                {plan.vectors.map((vec, i) => (
                  <div key={i} className="card" style={{ padding: 8, marginTop: 8 }}>
                    <div className="row">
                      <span className={`badge badge-${vec.severity}`}>{vec.severity}</span>
                      <strong>{vec.vuln}</strong>
                      <span className="muted">
                        in {vec.location}: <code>{vec.parameter}</code>
                      </span>
                    </div>
                    <table className="table" style={{ marginTop: 6 }}>
                      <thead>
                        <tr>
                          <th>Exp</th>
                          <th>Payload</th>
                          <th>Rationale</th>
                        </tr>
                      </thead>
                      <tbody>
                        {vec.payloads.map((p, j) => (
                          <tr key={j}>
                            <td>{p.exploitability}</td>
                            <td>
                              <code>{p.payload}</code>
                            </td>
                            <td>{p.rationale}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        {tab === "chain-scan" && (
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <div className="card" style={{ padding: 12 }}>
              <h4 style={{ marginTop: 0 }}>Chain scan: passive → active → report</h4>
              <p style={{ color: "var(--text-2)", marginTop: 0 }}>
                Target:{" "}
                <code className="kbd">
                  {latestFlow ? `${latestFlow.request.method} ${latestFlow.request.url}` : "(none)"}
                </code>
              </p>
              <button className="btn btn-primary" onClick={runChainScan} disabled={busy || !latestFlow}>
                {busy ? "Running…" : "Run chained scan"}
              </button>
            </div>
            {chain && (
              <div className="card" style={{ padding: 12 }}>
                <h4 style={{ marginTop: 0 }}>Risk score: {chain.risk_score}/100</h4>
                <p>{chain.summary}</p>
                {chain.steps.map((s, i) => (
                  <div key={i} className="card" style={{ padding: 8, marginTop: 8 }}>
                    <strong>{s.kind.toUpperCase()}</strong> · {s.title}
                    {s.issues.length > 0 && (
                      <ul>
                        {s.issues.map((it, j) => (
                          <li key={j}>{it}</li>
                        ))}
                      </ul>
                    )}
                    {s.notes && <p className="muted">{s.notes}</p>}
                  </div>
                ))}
                {chain.next_actions.length > 0 && (
                  <>
                    <h5>Next actions</h5>
                    <ul>
                      {chain.next_actions.map((a, i) => (
                        <li key={i}>{a}</li>
                      ))}
                    </ul>
                  </>
                )}
                <p style={{ color: "var(--text-2)", fontSize: 12 }}>
                  via <strong>{chain.provider}</strong> ({chain.model})
                </p>
              </div>
            )}
          </div>
        )}

        {tab === "fuzz" && (
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            <div className="card" style={{ padding: 12 }}>
              <h4 style={{ marginTop: 0 }}>AI fuzz mutator</h4>
              <label className="row">
                Attack class
                <select value={attackType} onChange={(e) => setAttackType(e.target.value)}>
                  {ATTACK_TYPES.map((t) => (
                    <option key={t} value={t}>
                      {t}
                    </option>
                  ))}
                </select>
              </label>
              <label className="row">
                Count
                <input
                  type="number"
                  min={1}
                  max={50}
                  value={count}
                  onChange={(e) => setCount(Math.max(1, Math.min(50, Number(e.target.value) || 1)))}
                />
              </label>
              <label className="row" style={{ alignItems: "stretch" }}>
                Seed payload
                <textarea
                  value={seed}
                  onChange={(e) => setSeed(e.target.value)}
                  rows={3}
                  style={{ fontFamily: "var(--font-mono)" }}
                />
              </label>
              <button className="btn btn-primary" onClick={runFuzz} disabled={busy}>
                {busy ? "Mutating…" : "Mutate"}
              </button>
            </div>
            {fuzz && (
              <div className="card" style={{ padding: 12 }}>
                <h4 style={{ marginTop: 0 }}>{fuzz.mutations.length} mutations</h4>
                <table className="table">
                  <thead>
                    <tr>
                      <th>Technique</th>
                      <th>Payload</th>
                      <th>Bypasses</th>
                    </tr>
                  </thead>
                  <tbody>
                    {fuzz.mutations.map((m, i) => (
                      <tr key={i}>
                        <td>{m.technique}</td>
                        <td>
                          <code>{m.payload}</code>
                        </td>
                        <td>{m.bypasses.join(", ")}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <p style={{ color: "var(--text-2)", fontSize: 12 }}>
                  via <strong>{fuzz.provider}</strong> ({fuzz.model})
                  {fuzz.fallbacks_tried.length > 0 && ` · fallbacks: ${fuzz.fallbacks_tried.join(", ")}`}
                </p>
              </div>
            )}
          </div>
        )}
      </div>
    </>
  );
}
