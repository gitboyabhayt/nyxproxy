import { useEffect, useMemo, useState } from "react";

import { useAppStore } from "@/state/store";
import { AiApi } from "@/tauri/api";
import type { AiChatMessage } from "@/tauri/types";

const SYSTEM_PROMPT = `You are NyxProxy, a senior application-security expert assistant inside an HTTP intercepting proxy. \
You help the user understand HTTP traffic, identify likely vulnerabilities, generate test payloads, and explain \
exploitation reasoning. Keep answers concise and concrete; prefer code blocks and bullet lists.`;

interface ChatMessage extends AiChatMessage {
  ts: number;
}

export function AiAssistantPage() {
  const providers = useAppStore((s) => s.providers);
  const toast = useAppStore((s) => s.toast);
  const reload = useAppStore((s) => s.reloadProviders);
  const history = useAppStore((s) => s.history);
  const [provider, setProvider] = useState<string>("");
  const [input, setInput] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!providers) reload();
  }, [providers, reload]);

  const available = providers?.providers.filter((p) => p.available) ?? [];
  const activeProvider =
    provider || providers?.default || available[0]?.name || "";

  const messagesForApi: AiChatMessage[] = useMemo(
    () => [
      { role: "system", content: SYSTEM_PROMPT },
      ...messages.map((m) => ({ role: m.role, content: m.content })),
    ],
    [messages]
  );

  const send = async (override?: string) => {
    const content = (override ?? input).trim();
    if (!content) return;
    const newUser: ChatMessage = { role: "user", content, ts: Date.now() };
    setMessages((prev) => [...prev, newUser]);
    setInput("");
    setBusy(true);
    try {
      const resp = await AiApi.chat({
        messages: [...messagesForApi, { role: "user", content }],
        provider: activeProvider || null,
        model: null,
        temperature: 0.4,
        max_tokens: 1024,
      });
      const assistant: ChatMessage = {
        role: "assistant",
        content: resp.choices[0]?.message.content ?? "(no content)",
        ts: Date.now(),
      };
      setMessages((prev) => [...prev, assistant]);
    } catch (err) {
      toast("error", `AI request failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const explainLastFlow = async () => {
    const flow = history[0]?.flow;
    if (!flow) {
      toast("warning", "No captured flow available yet.");
      return;
    }
    setBusy(true);
    try {
      const resp = await AiApi.analyzeRequest({
        request: {
          method: flow.request.method,
          url: flow.request.url,
          http_version: flow.request.http_version,
          headers: flow.request.headers.reduce<Record<string, string>>((acc, h) => {
            acc[h.name] = h.value;
            return acc;
          }, {}),
          body: null,
        },
        response: flow.response
          ? {
              status: flow.response.status,
              http_version: flow.response.http_version,
              headers: flow.response.headers.reduce<Record<string, string>>((acc, h) => {
                acc[h.name] = h.value;
                return acc;
              }, {}),
              body: null,
            }
          : null,
        provider: activeProvider || null,
      });
      setMessages((prev) => [
        ...prev,
        {
          role: "user",
          content: `Explain this flow: ${flow.request.method} ${flow.request.url}`,
          ts: Date.now(),
        },
        { role: "assistant", content: resp.content, ts: Date.now() },
      ]);
    } catch (err) {
      toast("error", `Analyze failed: ${err}`);
    } finally {
      setBusy(false);
    }
  };

  const findVulnsForLastFlow = async () => {
    const flow = history[0]?.flow;
    if (!flow) {
      toast("warning", "No captured flow available yet.");
      return;
    }
    setBusy(true);
    try {
      const resp = await AiApi.findVulns({
        request: {
          method: flow.request.method,
          url: flow.request.url,
          http_version: flow.request.http_version,
          headers: flow.request.headers.reduce<Record<string, string>>((acc, h) => {
            acc[h.name] = h.value;
            return acc;
          }, {}),
          body: null,
        },
        response: null,
        provider: activeProvider || null,
      });
      setMessages((prev) => [
        ...prev,
        {
          role: "user",
          content: `Find vulnerabilities in ${flow.request.method} ${flow.request.url}`,
          ts: Date.now(),
        },
        { role: "assistant", content: resp.content, ts: Date.now() },
      ]);
    } catch (err) {
      toast("error", `Vuln scan failed: ${err}`);
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
          Refresh providers
        </button>
        <span className="grow" />
        <button className="btn" onClick={explainLastFlow} disabled={busy}>
          Explain last flow
        </button>
        <button className="btn" onClick={findVulnsForLastFlow} disabled={busy}>
          Find vulnerabilities
        </button>
      </div>
      <div className="main-content" style={{ overflow: "hidden" }}>
        <div style={{ flex: 1, overflow: "auto", padding: 20 }}>
          {messages.length === 0 && (
            <div className="empty-state">
              <h3>NyxProxy AI Assistant</h3>
              <p>
                Ask anything about the captured flows, request a payload, explain a status code, or use one of the
                presets in the toolbar. The chat is routed through the backend gateway, so it works with whichever
                providers you configured (Groq, OpenRouter, HuggingFace, NVIDIA, Cloudflare, GitHub Models, Gemini,
                Bytez, or Ollama).
              </p>
            </div>
          )}
          {messages.map((m, i) => (
            <div
              key={i}
              style={{
                display: "flex",
                gap: 12,
                margin: "0 0 12px 0",
              }}
            >
              <div
                style={{
                  width: 36,
                  height: 36,
                  borderRadius: 6,
                  background: m.role === "user" ? "var(--bg-3)" : "var(--accent-soft)",
                  color: m.role === "user" ? "var(--text)" : "var(--accent)",
                  fontWeight: 600,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  flexShrink: 0,
                }}
              >
                {m.role === "user" ? "U" : "AI"}
              </div>
              <pre
                className="code"
                style={{ flex: 1, margin: 0, whiteSpace: "pre-wrap", background: "var(--bg-1)" }}
              >
                {m.content}
              </pre>
            </div>
          ))}
        </div>
        <div className="toolbar" style={{ borderTop: "1px solid var(--border)", padding: 10 }}>
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Ask the AI…"
            rows={2}
            style={{ flex: 1, resize: "none", padding: "6px 8px" }}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                e.preventDefault();
                send();
              }
            }}
          />
          <button className="btn primary" onClick={() => send()} disabled={busy}>
            {busy ? "Sending…" : "Send (⌃↵)"}
          </button>
        </div>
      </div>
    </>
  );
}
