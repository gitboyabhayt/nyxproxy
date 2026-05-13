import { Plug } from "lucide-react";

export function ExtenderPage() {
  return (
    <div className="section" style={{ overflow: "auto" }}>
      <div>
        <h2>Extender</h2>
        <p style={{ color: "var(--text-dim)", marginTop: 4 }}>
          NyxProxy will support Python and JavaScript extensions in Phase 2 — exposing the captured-flow API, proxy
          intercept hooks, an issue-store, and direct access to the AI gateway. The extension manifest format and
          sample plugins are already drafted at <code className="code" style={{ padding: "0 4px" }}>docs/extender.md</code>.
        </p>
      </div>
      <div className="cards">
        {EXTENSIONS.map((ext) => (
          <div className="card" key={ext.name}>
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <Plug size={18} />
              <strong style={{ flex: 1 }}>{ext.name}</strong>
              <span className="pill">{ext.status}</span>
            </div>
            <div className="sub" style={{ marginTop: 8 }}>{ext.description}</div>
          </div>
        ))}
      </div>
    </div>
  );
}

const EXTENSIONS = [
  {
    name: "AI Active Scanner",
    status: "Phase 2",
    description:
      "Drives the AI gateway against every in-scope flow, surfacing reflected/stored XSS, SQLi, IDOR, SSRF, auth-bypass, and template-injection candidates with reproducer payloads.",
  },
  {
    name: "JWT Auditor",
    status: "Phase 2",
    description:
      "Detects JWTs in cookies/headers, decodes and analyses claims, attempts none-alg, kid-injection, and weak-secret HMAC attacks.",
  },
  {
    name: "GraphQL Inspector",
    status: "Phase 2",
    description:
      "Auto-introspects GraphQL endpoints, builds a schema map, and lets you mutate variables directly from the Repeater.",
  },
  {
    name: "Collaborator (OAST)",
    status: "Phase 3",
    description:
      "Self-hosted out-of-band server with DNS/HTTP capture, used to detect blind injection vulnerabilities.",
  },
];
