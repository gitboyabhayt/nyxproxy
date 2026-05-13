import { useMemo, useState } from "react";

import { base64ToText, tryParseJson } from "@/lib/codec";
import { parseQuery, rawRequest, rawResponse } from "@/lib/http";
import type { CapturedRequest, CapturedResponse } from "@/tauri/types";

type Side = "request" | "response";

interface ViewerProps {
  side: Side;
  request?: CapturedRequest;
  response?: CapturedResponse | null;
}

const SUB_TABS = ["Pretty", "Raw", "Headers", "Params", "Body", "Hex"] as const;

type SubTab = (typeof SUB_TABS)[number];

export function RequestViewer({ side, request, response }: ViewerProps) {
  const [tab, setTab] = useState<SubTab>("Pretty");

  const raw = useMemo(() => {
    if (side === "request" && request) return rawRequest(request);
    if (side === "response" && response) return rawResponse(response);
    return "";
  }, [side, request, response]);

  const body = useMemo(() => {
    if (side === "request" && request) return base64ToText(request.body_b64);
    if (side === "response" && response) return base64ToText(response.body_b64);
    return "";
  }, [side, request, response]);

  const headers = useMemo(() => {
    if (side === "request" && request) return request.headers;
    if (side === "response" && response) return response.headers;
    return [];
  }, [side, request, response]);

  const params = useMemo(() => {
    if (side === "request" && request) return parseQuery(request.url).params;
    return [];
  }, [side, request]);

  const pretty = useMemo(() => {
    const parsed = tryParseJson(body);
    if (parsed !== null) return JSON.stringify(parsed, null, 2);
    return body;
  }, [body]);

  const hex = useMemo(() => {
    if (!body) return "";
    const bytes = new TextEncoder().encode(body);
    const lines: string[] = [];
    for (let i = 0; i < bytes.length; i += 16) {
      const chunk = bytes.slice(i, i + 16);
      const hexs = Array.from(chunk)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join(" ");
      const ascii = Array.from(chunk)
        .map((b) => (b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : "."))
        .join("");
      lines.push(
        `${i.toString(16).padStart(8, "0")}  ${hexs.padEnd(48, " ")}  ${ascii}`
      );
    }
    return lines.join("\n");
  }, [body]);

  if (side === "response" && !response) {
    return (
      <div className="panel-body">
        <div className="empty-state">
          <h3>No response yet</h3>
          <p>The proxy will populate this pane once the upstream replies.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="panel-body">
      <div className="sub-tabs">
        {SUB_TABS.map((t) => (
          <div
            key={t}
            className={`sub-tab ${tab === t ? "active" : ""}`}
            onClick={() => setTab(t)}
          >
            {t}
          </div>
        ))}
      </div>
      <div className="main-content" style={{ overflow: "auto" }}>
        {tab === "Pretty" && (
          <pre className="code" style={{ margin: 0, border: "none" }}>
            {pretty || raw}
          </pre>
        )}
        {tab === "Raw" && (
          <pre className="code" style={{ margin: 0, border: "none" }}>
            {raw}
          </pre>
        )}
        {tab === "Headers" && (
          <div style={{ padding: 12 }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th style={{ width: "30%" }}>Name</th>
                  <th>Value</th>
                </tr>
              </thead>
              <tbody>
                {headers.map((h, i) => (
                  <tr key={`${h.name}-${i}`}>
                    <td>{h.name}</td>
                    <td style={{ whiteSpace: "pre-wrap" }}>{h.value}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {tab === "Params" && (
          <div style={{ padding: 12 }}>
            {params.length === 0 ? (
              <div className="notice">No query parameters in this URL.</div>
            ) : (
              <table className="data-table">
                <thead>
                  <tr>
                    <th style={{ width: "30%" }}>Key</th>
                    <th>Value</th>
                  </tr>
                </thead>
                <tbody>
                  {params.map((p, i) => (
                    <tr key={i}>
                      <td>{p.key}</td>
                      <td style={{ whiteSpace: "pre-wrap" }}>{p.value}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        )}
        {tab === "Body" && (
          <pre className="code" style={{ margin: 0, border: "none" }}>
            {body || "<empty body>"}
          </pre>
        )}
        {tab === "Hex" && (
          <pre className="code" style={{ margin: 0, border: "none" }}>
            {hex || "<empty body>"}
          </pre>
        )}
      </div>
    </div>
  );
}
