import { useMemo, useState } from "react";
import { Plus, X } from "lucide-react";

import { RequestViewer } from "@/components/RequestViewer";
import { SplitPane } from "@/components/SplitPane";
import { textToBase64 } from "@/lib/codec";
import { useAppStore } from "@/state/store";
import { Http3Api, RepeaterApi } from "@/tauri/api";
import type { CapturedResponse } from "@/tauri/types";

export function RepeaterPage() {
  const drafts = useAppStore((s) => s.repeaterDrafts);
  const upsert = useAppStore((s) => s.upsertRepeaterDraft);
  const remove = useAppStore((s) => s.removeRepeaterDraft);
  const toast = useAppStore((s) => s.toast);

  const draftIds = useMemo(() => Object.keys(drafts), [drafts]);
  const [activeId, setActiveId] = useState<string | null>(draftIds[0] ?? null);

  if (!activeId && draftIds[0]) setActiveId(draftIds[0]);
  const active = activeId ? drafts[activeId] : null;

  const newDraft = () => {
    const id = `draft-${Date.now()}`;
    upsert({
      id,
      title: `Untitled`,
      method: "GET",
      url: "https://httpbin.org/get",
      headers: [
        { name: "Host", value: "httpbin.org" },
        { name: "User-Agent", value: "NyxProxy/0.1" },
      ],
      body: "",
      follow_redirects: false,
      insecure: false,
    });
    setActiveId(id);
  };

  const send = async () => {
    if (!active) return;
    try {
      const resp = await RepeaterApi.send({
        method: active.method,
        url: active.url,
        headers: active.headers,
        body_b64: textToBase64(active.body),
        follow_redirects: active.follow_redirects,
        insecure: active.insecure,
      });
      upsert({ ...active, lastResponse: resp, lastError: null });
    } catch (err) {
      upsert({ ...active, lastResponse: null, lastError: String(err) });
      toast("error", `Send failed: ${err}`);
    }
  };

  /** Send the draft over HTTP/3 (QUIC) and surface the response as a synthesised CapturedResponse. */
  const sendH3 = async () => {
    if (!active) return;
    try {
      const h3 = await Http3Api.send({
        method: active.method,
        url: active.url,
        headers: active.headers.map((h) => [h.name, h.value] as [string, string]),
        body_b64: textToBase64(active.body),
      });
      const synthetic: CapturedResponse = {
        status: h3.status,
        http_version: h3.http_version,
        reason: "",
        headers: h3.headers.map(([name, value]) => ({ name, value })),
        body_b64: h3.body_b64,
        body_size: h3.body_size,
        elapsed_ms: h3.elapsed_ms,
      };
      upsert({ ...active, lastResponse: synthetic, lastError: null });
      toast("info", `HTTP/3 ${h3.status} in ${h3.elapsed_ms}ms`);
    } catch (err) {
      upsert({ ...active, lastResponse: null, lastError: String(err) });
      toast("error", `HTTP/3 send failed: ${err}`);
    }
  };

  if (draftIds.length === 0) {
    return (
      <div className="empty-state">
        <h3>No Repeater tabs</h3>
        <p>Send a flow from Proxy → HTTP history, or open a fresh tab.</p>
        <button className="btn primary" onClick={newDraft}>
          <Plus size={14} /> New tab
        </button>
      </div>
    );
  }

  return (
    <>
      <div className="main-tabs">
        {draftIds.map((id) => {
          const d = drafts[id];
          if (!d) return null;
          return (
            <div
              key={id}
              className={`tab ${activeId === id ? "active" : ""}`}
              onClick={() => setActiveId(id)}
              style={{ display: "flex", gap: 6, alignItems: "center" }}
            >
              <span>{d.title}</span>
              <X
                size={12}
                onClick={(e) => {
                  e.stopPropagation();
                  remove(id);
                  if (activeId === id) setActiveId(null);
                }}
              />
            </div>
          );
        })}
        <div
          className="tab"
          onClick={newDraft}
          style={{ display: "flex", alignItems: "center", gap: 4 }}
        >
          <Plus size={12} /> New
        </div>
      </div>
      {active && (
        <SplitPane
          storageKey="repeater-vertical"
          direction="vertical"
          initialSize={0.5}
          first={
            <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
              <div className="panel-header">
                <span style={{ flex: 1 }}>Request</span>
                <button className="btn primary small" onClick={send}>
                  Send
                </button>
                <button
                  className="btn small"
                  onClick={sendH3}
                  title="Send via HTTP/3 (QUIC). Target must be https:// and serve h3."
                >
                  Send /h3
                </button>
              </div>
              <div className="toolbar">
                <select
                  value={active.method}
                  onChange={(e) => upsert({ ...active, method: e.target.value })}
                  style={{ width: 100 }}
                >
                  {[
                    "GET",
                    "POST",
                    "PUT",
                    "PATCH",
                    "DELETE",
                    "HEAD",
                    "OPTIONS",
                  ].map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
                <input
                  value={active.url}
                  onChange={(e) => upsert({ ...active, url: e.target.value })}
                  className="grow"
                />
                <label
                  style={{
                    display: "flex",
                    gap: 6,
                    alignItems: "center",
                    color: "var(--text-dim)",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={active.follow_redirects}
                    onChange={(e) =>
                      upsert({ ...active, follow_redirects: e.target.checked })
                    }
                  />
                  Follow redirects
                </label>
                <label
                  style={{
                    display: "flex",
                    gap: 6,
                    alignItems: "center",
                    color: "var(--text-dim)",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={active.insecure}
                    onChange={(e) => upsert({ ...active, insecure: e.target.checked })}
                  />
                  TLS insecure
                </label>
              </div>
              <div className="panel-body" style={{ overflow: "auto", padding: 10 }}>
                <h3 style={{ margin: "4px 0 6px 0", fontSize: 11, color: "var(--text-muted)" }}>
                  HEADERS
                </h3>
                <HeadersEditor
                  headers={active.headers}
                  onChange={(headers) => upsert({ ...active, headers })}
                />
                <h3 style={{ margin: "10px 0 6px 0", fontSize: 11, color: "var(--text-muted)" }}>BODY</h3>
                <textarea
                  className="code-input"
                  value={active.body}
                  onChange={(e) => upsert({ ...active, body: e.target.value })}
                  placeholder="Raw request body"
                  rows={8}
                />
              </div>
            </div>
          }
          second={
            <div className="panel" style={{ height: "100%", border: "none", borderRadius: 0 }}>
              <div className="panel-header">
                Response{" "}
                {active.lastResponse && (
                  <span className="pill">{active.lastResponse.status}</span>
                )}
                {active.lastResponse && (
                  <span style={{ marginLeft: "auto", color: "var(--text-muted)" }}>
                    {active.lastResponse.elapsed_ms} ms · {active.lastResponse.body_size} bytes
                  </span>
                )}
              </div>
              {active.lastError && (
                <div className="banner error">{active.lastError}</div>
              )}
              <RepeaterResponse response={active.lastResponse ?? null} />
            </div>
          }
        />
      )}
    </>
  );
}

function HeadersEditor({
  headers,
  onChange,
}: {
  headers: Array<{ name: string; value: string }>;
  onChange: (next: Array<{ name: string; value: string }>) => void;
}) {
  return (
    <div>
      {headers.map((h, i) => (
        <div key={i} style={{ display: "flex", gap: 6, marginBottom: 4 }}>
          <input
            value={h.name}
            onChange={(e) =>
              onChange(headers.map((row, ix) => (ix === i ? { ...row, name: e.target.value } : row)))
            }
            style={{ width: 240 }}
          />
          <input
            value={h.value}
            onChange={(e) =>
              onChange(headers.map((row, ix) => (ix === i ? { ...row, value: e.target.value } : row)))
            }
            style={{ flex: 1 }}
          />
          <button
            className="btn ghost small"
            onClick={() => onChange(headers.filter((_, ix) => ix !== i))}
          >
            <X size={12} />
          </button>
        </div>
      ))}
      <button
        className="btn ghost small"
        onClick={() => onChange([...headers, { name: "", value: "" }])}
      >
        <Plus size={12} /> Add header
      </button>
    </div>
  );
}

function RepeaterResponse({ response }: { response: CapturedResponse | null }) {
  if (!response) {
    return (
      <div className="empty-state">
        <p>Press <strong>Send</strong> to fire the request and see the response here.</p>
      </div>
    );
  }
  return <RequestViewer side="response" response={response} />;
}
