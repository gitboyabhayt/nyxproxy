import { useState } from "react";

import { SplitPane } from "@/components/SplitPane";
import { useAppStore } from "@/state/store";
import { DecoderApi } from "@/tauri/api";
import type { Codec, DecoderSmartResult } from "@/tauri/types";

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

export function DecoderPage() {
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
