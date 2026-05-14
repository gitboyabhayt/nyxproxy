import { useState } from "react";

import { SplitPane } from "@/components/SplitPane";
import { useAppStore } from "@/state/store";
import { DecoderApi, JwtApi } from "@/tauri/api";
import type {
  Codec,
  DecoderSmartResult,
  JwtBruteResult,
  JwtDecoded,
  JwtFinding,
} from "@/tauri/types";

const CODECS: Array<{ id: Codec; label: string }> = [
  { id: "base64", label: "Base64" },
  { id: "base64_url", label: "Base64 URL-safe" },
  { id: "url", label: "URL" },
  { id: "html", label: "HTML entities" },
  { id: "hex", label: "Hex" },
  { id: "ascii", label: "ASCII (no-op)" },
  { id: "gzip", label: "Gzip" },
  { id: "deflate", label: "Deflate (zlib)" },
  { id: "zstd", label: "Zstandard" },
];

type Tab = "codecs" | "jwt";

export function DecoderPage() {
  const [tab, setTab] = useState<Tab>("codecs");
  return (
    <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
      <div className="panel-header" style={{ gap: 8 }}>
        <button
          className={`btn small ${tab === "codecs" ? "primary" : "ghost"}`}
          onClick={() => setTab("codecs")}
        >
          Codecs
        </button>
        <button
          className={`btn small ${tab === "jwt" ? "primary" : "ghost"}`}
          onClick={() => setTab("jwt")}
        >
          JWT toolkit
        </button>
      </div>
      <div className="panel-body" style={{ padding: 0, overflow: "hidden" }}>
        {tab === "codecs" ? <CodecsTab /> : <JwtTab />}
      </div>
    </div>
  );
}

function CodecsTab() {
  const toast = useAppStore((s) => s.toast);
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [smart, setSmart] = useState<DecoderSmartResult[] | null>(null);
  const [codec, setCodec] = useState<Codec>("base64");

  const run = async (fn: "encode" | "decode") => {
    try {
      const out =
        fn === "encode"
          ? await DecoderApi.encode(codec, input)
          : await DecoderApi.decode(codec, input);
      setOutput(out);
      setSmart(null);
    } catch (err) {
      toast("error", `${fn} failed: ${err}`);
    }
  };

  const runSmart = async () => {
    try {
      const results = await DecoderApi.smart(input);
      setSmart(results);
      setOutput("");
    } catch (err) {
      toast("error", `Smart decode failed: ${err}`);
    }
  };

  return (
    <SplitPane
      storageKey="decoder"
      initialSize={0.5}
      first={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">Input</div>
          <div className="toolbar">
            <select value={codec} onChange={(e) => setCodec(e.target.value as Codec)}>
              {CODECS.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.label}
                </option>
              ))}
            </select>
            <button className="btn" onClick={() => run("encode")}>
              Encode
            </button>
            <button className="btn" onClick={() => run("decode")}>
              Decode
            </button>
            <button className="btn primary" onClick={runSmart}>
              Smart decode
            </button>
          </div>
          <div className="panel-body" style={{ padding: 10 }}>
            <textarea
              className="code-input"
              style={{ flex: 1, minHeight: 300 }}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder="Paste anything — base64, hex, URL-encoded, gzipped, JWT, etc."
            />
          </div>
        </div>
      }
      second={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">Output</div>
          <div className="panel-body" style={{ padding: 10, overflow: "auto" }}>
            {smart ? (
              <SmartResultsTable results={smart} onUseAsInput={(t) => setInput(t)} />
            ) : (
              <pre className="code" style={{ flex: 1, margin: 0 }}>{output}</pre>
            )}
          </div>
        </div>
      }
    />
  );
}

const DEFAULT_TOKEN =
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

const TOP_SECRETS = [
  "secret",
  "password",
  "admin",
  "test",
  "your-256-bit-secret",
  "changeme",
  "p@ssw0rd",
  "qwerty",
  "letmein",
  "default",
];

function JwtTab() {
  const toast = useAppStore((s) => s.toast);
  const [token, setToken] = useState(DEFAULT_TOKEN);
  const [decoded, setDecoded] = useState<JwtDecoded | null>(null);
  const [findings, setFindings] = useState<JwtFinding[]>([]);
  const [secret, setSecret] = useState("your-256-bit-secret");
  const [bruteList, setBruteList] = useState(TOP_SECRETS.join("\n"));
  const [bruteResult, setBruteResult] = useState<JwtBruteResult | null>(null);

  const onDecode = async () => {
    try {
      const d = await JwtApi.decode(token.trim());
      setDecoded(d);
      const f = await JwtApi.analyze(token.trim());
      setFindings(f);
    } catch (err) {
      toast("error", `JWT decode failed: ${err}`);
      setDecoded(null);
      setFindings([]);
    }
  };

  const onResign = async () => {
    if (!decoded) return;
    try {
      const next = await JwtApi.encodeHs256(decoded.header, decoded.payload, secret);
      setToken(next);
      toast("info", "Re-signed token written to input.");
    } catch (err) {
      toast("error", `JWT encode failed: ${err}`);
    }
  };

  const onAlgNone = async () => {
    if (!decoded) return;
    try {
      const next = await JwtApi.encodeNone(decoded.header, decoded.payload);
      setToken(next);
      toast("warning", "Generated alg=none token (test target acceptance).");
    } catch (err) {
      toast("error", `JWT encode failed: ${err}`);
    }
  };

  const onBrute = async () => {
    try {
      const candidates = bruteList
        .split(/\r?\n/)
        .map((s) => s.trim())
        .filter(Boolean);
      const r = await JwtApi.bruteHs256(token.trim(), candidates);
      setBruteResult(r);
      if (r.secret) toast("info", `Found secret: ${r.secret}`);
      else toast("warning", `Tried ${r.tried} candidates, no match.`);
    } catch (err) {
      toast("error", `Brute force failed: ${err}`);
    }
  };

  return (
    <SplitPane
      storageKey="decoder-jwt"
      initialSize={0.5}
      first={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">Token</div>
          <div className="toolbar">
            <button className="btn primary" onClick={onDecode}>
              Decode & analyse
            </button>
            <button className="btn" onClick={onResign} disabled={!decoded}>
              Re-sign HS256
            </button>
            <button className="btn danger" onClick={onAlgNone} disabled={!decoded}>
              alg=none
            </button>
          </div>
          <div className="panel-body" style={{ padding: 10, gap: 10 }}>
            <textarea
              className="code-input"
              style={{ minHeight: 120 }}
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="Paste a JSON Web Token (header.payload.signature)…"
            />
            <label style={{ fontSize: 12, color: "var(--text-muted)" }}>
              HS256 secret (used when re-signing)
            </label>
            <input
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
              placeholder="secret"
              spellCheck={false}
            />
            <details>
              <summary style={{ cursor: "pointer", fontSize: 12, color: "var(--text-muted)" }}>
                Brute-force HS256 secret
              </summary>
              <div style={{ marginTop: 8, display: "flex", flexDirection: "column", gap: 6 }}>
                <textarea
                  className="code-input"
                  style={{ minHeight: 100 }}
                  value={bruteList}
                  onChange={(e) => setBruteList(e.target.value)}
                  placeholder="One candidate per line. Top-10 weak secrets pre-filled."
                />
                <div>
                  <button className="btn" onClick={onBrute}>
                    Run brute force
                  </button>
                </div>
                {bruteResult && (
                  <div className="banner info">
                    Tried {bruteResult.tried} · {bruteResult.elapsed_ms} ms ·{" "}
                    {bruteResult.secret
                      ? `secret = ${bruteResult.secret}`
                      : "no match"}
                  </div>
                )}
              </div>
            </details>
          </div>
        </div>
      }
      second={
        <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
          <div className="panel-header">Decoded</div>
          <div className="panel-body" style={{ padding: 10, overflow: "auto", gap: 10 }}>
            {!decoded ? (
              <div style={{ color: "var(--text-muted)", fontSize: 12 }}>
                Press <b>Decode &amp; analyse</b> to inspect the token.
              </div>
            ) : (
              <>
                <div style={{ fontSize: 11, color: "var(--text-muted)" }}>HEADER</div>
                <pre className="code" style={{ margin: 0 }}>
                  {JSON.stringify(decoded.header, null, 2)}
                </pre>
                <div style={{ fontSize: 11, color: "var(--text-muted)" }}>PAYLOAD</div>
                <pre className="code" style={{ margin: 0 }}>
                  {JSON.stringify(decoded.payload, null, 2)}
                </pre>
                <div style={{ fontSize: 11, color: "var(--text-muted)" }}>SIGNATURE</div>
                <pre className="code" style={{ margin: 0, wordBreak: "break-all" }}>
                  {decoded.signature_b64 || "(empty — alg=none)"}
                </pre>
                {findings.length > 0 && (
                  <>
                    <div style={{ fontSize: 11, color: "var(--text-muted)" }}>
                      FINDINGS ({findings.length})
                    </div>
                    {findings.map((f, i) => (
                      <div key={i} className={`banner ${severityToBanner(f.severity)}`}>
                        <b>{labelKind(f.kind)}</b> — {f.detail}
                      </div>
                    ))}
                  </>
                )}
              </>
            )}
          </div>
        </div>
      }
    />
  );
}

function severityToBanner(sev: JwtFinding["severity"]): string {
  switch (sev) {
    case "high":
      return "error";
    case "medium":
      return "warning";
    case "low":
      return "info";
    default:
      return "info";
  }
}

function labelKind(kind: JwtFinding["kind"]): string {
  return kind
    .split("_")
    .map((s) => s.charAt(0).toUpperCase() + s.slice(1))
    .join(" ");
}

function SmartResultsTable({
  results,
  onUseAsInput,
}: {
  results: DecoderSmartResult[];
  onUseAsInput: (text: string) => void;
}) {
  return (
    <table className="data-table">
      <thead>
        <tr>
          <th style={{ width: 120 }}>Codec</th>
          <th style={{ width: 80 }}>Status</th>
          <th>Result</th>
          <th style={{ width: 80 }}></th>
        </tr>
      </thead>
      <tbody>
        {results.map((r, i) => (
          <tr key={`${r.codec}-${i}`}>
            <td>{r.codec}</td>
            <td>
              <span className={`status-badge ${r.success ? "status-2xx" : "status-4xx"}`}>
                {r.success ? "OK" : "FAIL"}
              </span>
            </td>
            <td>
              <pre
                className="code"
                style={{ margin: 0, maxHeight: 80, whiteSpace: "pre-wrap" }}
              >
                {r.output}
              </pre>
            </td>
            <td>
              {r.success && (
                <button
                  className="btn ghost small"
                  onClick={() => onUseAsInput(r.output)}
                >
                  Use →
                </button>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
