import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDownLeft,
  ArrowUpRight,
  Plug2,
  RefreshCw,
  Send,
} from "lucide-react";

import { useAppStore } from "@/state/store";
import { WebSocketApi, type WsEvent } from "@/tauri/api";
import type {
  WsDirection,
  WsFrame,
  WsOpcode,
  WsSession,
} from "@/tauri/types";

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KiB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MiB`;
}

function decodeFramePreview(frame: WsFrame): string {
  if (frame.text != null) return frame.text;
  try {
    const bytes = Uint8Array.from(atob(frame.payload_b64), (c) => c.charCodeAt(0));
    let s = "";
    for (let i = 0; i < bytes.length && i < 80; i++) {
      const b = bytes[i] ?? 0;
      s += b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".";
    }
    return s + (bytes.length > 80 ? "…" : "");
  } catch {
    return "<binary>";
  }
}

interface FrameRowProps {
  frame: WsFrame;
  active: boolean;
  onSelect: () => void;
}

function FrameRow({ frame, active, onSelect }: FrameRowProps) {
  const isC2S = frame.direction === "client_to_server";
  return (
    <tr
      onClick={onSelect}
      className={`ws-row ${active ? "selected" : ""}`}
      title={frame.captured_at}
    >
      <td>{new Date(frame.captured_at).toLocaleTimeString()}</td>
      <td>
        {isC2S ? (
          <ArrowUpRight size={14} aria-label="client → server" />
        ) : (
          <ArrowDownLeft size={14} aria-label="server → client" />
        )}
      </td>
      <td>{frame.opcode}</td>
      <td>{formatBytes(frame.payload_size)}</td>
      <td className="ws-preview">
        {frame.injected ? <span className="ws-tag">replay</span> : null}
        <span>{decodeFramePreview(frame)}</span>
      </td>
    </tr>
  );
}

export function WebSocketsPage() {
  const toast = useAppStore((s) => s.toast);
  const [sessions, setSessions] = useState<WsSession[]>([]);
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(null);
  const [frames, setFrames] = useState<WsFrame[]>([]);
  const [selectedFrame, setSelectedFrame] = useState<WsFrame | null>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const [replayText, setReplayText] = useState("");
  const [replayDir, setReplayDir] = useState<WsDirection>("client_to_server");
  const [replayOpcode, setReplayOpcode] = useState<WsOpcode>("text");
  const framesRef = useRef<HTMLDivElement | null>(null);

  const refreshSessions = useCallback(async () => {
    try {
      const list = await WebSocketApi.listSessions();
      setSessions(list);
      // If nothing selected, select the most recent.
      if (!selectedSessionId && list.length > 0 && list[0]) {
        setSelectedSessionId(list[0].id);
      }
    } catch (err) {
      toast("error", `WS list failed: ${err}`);
    }
  }, [selectedSessionId, toast]);

  const refreshFrames = useCallback(async (id: string) => {
    try {
      const list = await WebSocketApi.frames(id);
      setFrames(list);
    } catch (err) {
      toast("error", `WS frames failed: ${err}`);
    }
  }, [toast]);

  useEffect(() => {
    void refreshSessions();
  }, [refreshSessions]);

  useEffect(() => {
    if (selectedSessionId) {
      void refreshFrames(selectedSessionId);
    }
  }, [selectedSessionId, refreshFrames]);

  // Live updates via Tauri event stream.
  useEffect(() => {
    let unsub: (() => void) | undefined;
    let mounted = true;
    (async () => {
      try {
        unsub = await WebSocketApi.subscribe((event: WsEvent) => {
          if (!mounted) return;
          if (event.kind === "session_started") {
            setSessions((prev) => [event.session, ...prev]);
            if (!selectedSessionId) setSelectedSessionId(event.session.id);
          } else if (event.kind === "session_ended") {
            setSessions((prev) =>
              prev.map((s) => (s.id === event.session.id ? event.session : s))
            );
          } else if (event.kind === "frame") {
            if (event.frame.session_id === selectedSessionId) {
              setFrames((prev) => [...prev, event.frame]);
            }
            setSessions((prev) =>
              prev.map((s) =>
                s.id === event.frame.session_id
                  ? { ...s, frame_count: s.frame_count + 1 }
                  : s
              )
            );
          }
        });
      } catch {
        // No-op in browser preview.
      }
    })();
    return () => {
      mounted = false;
      unsub?.();
    };
  }, [selectedSessionId]);

  useEffect(() => {
    if (autoScroll && framesRef.current) {
      framesRef.current.scrollTop = framesRef.current.scrollHeight;
    }
  }, [frames, autoScroll]);

  const submitReplay = useCallback(async () => {
    if (!selectedSessionId) {
      toast("warning", "No active WebSocket session");
      return;
    }
    try {
      await WebSocketApi.replay({
        sessionId: selectedSessionId,
        direction: replayDir,
        opcode: replayOpcode,
        text: replayOpcode === "text" ? replayText : undefined,
      });
      setReplayText("");
      toast("info", "Frame injected");
    } catch (err) {
      toast("error", `Replay failed: ${err}`);
    }
  }, [replayDir, replayOpcode, replayText, selectedSessionId, toast]);

  const selectedSession = useMemo(
    () => sessions.find((s) => s.id === selectedSessionId) ?? null,
    [sessions, selectedSessionId]
  );

  return (
    <div className="page ws-page">
      <div className="page-header">
        <h1>
          <Plug2 size={18} /> WebSockets
        </h1>
        <button className="btn-secondary" onClick={() => void refreshSessions()}>
          <RefreshCw size={14} /> Refresh
        </button>
      </div>

      <div className="ws-layout">
        <aside className="ws-sessions">
          <h3>Sessions ({sessions.length})</h3>
          {sessions.length === 0 ? (
            <p className="muted">
              No WebSocket sessions captured yet. Start the proxy and have a
              client open a <code>wss://</code> connection through it.
            </p>
          ) : (
            <ul className="ws-session-list">
              {sessions.map((s) => (
                <li
                  key={s.id}
                  className={`ws-session-item ${selectedSessionId === s.id ? "active" : ""}`}
                  onClick={() => setSelectedSessionId(s.id)}
                >
                  <div className="ws-session-host">{s.host}</div>
                  <div className="ws-session-meta">
                    {s.frame_count} frames {s.ended_at ? "· closed" : "· live"}
                  </div>
                  <div className="ws-session-url" title={s.url}>{s.url}</div>
                </li>
              ))}
            </ul>
          )}
        </aside>

        <section className="ws-frames-pane">
          {selectedSession ? (
            <>
              <div className="ws-session-banner">
                <span>{selectedSession.url}</span>
                <span className="muted">
                  started {new Date(selectedSession.started_at).toLocaleString()}
                  {selectedSession.ended_at
                    ? ` · ended ${new Date(selectedSession.ended_at).toLocaleString()}`
                    : ""}
                  {selectedSession.close_code != null
                    ? ` · close ${selectedSession.close_code}`
                    : ""}
                </span>
              </div>
              <div className="ws-frames-table-wrap" ref={framesRef}>
                <table className="ws-frames-table">
                  <thead>
                    <tr>
                      <th>Time</th>
                      <th>Dir</th>
                      <th>Op</th>
                      <th>Size</th>
                      <th>Preview</th>
                    </tr>
                  </thead>
                  <tbody>
                    {frames.map((f) => (
                      <FrameRow
                        key={f.id}
                        frame={f}
                        active={selectedFrame?.id === f.id}
                        onSelect={() => setSelectedFrame(f)}
                      />
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="ws-frames-foot">
                <label>
                  <input
                    type="checkbox"
                    checked={autoScroll}
                    onChange={(e) => setAutoScroll(e.target.checked)}
                  />
                  Follow tail
                </label>
                <span className="muted">{frames.length} frames</span>
              </div>
            </>
          ) : (
            <div className="muted ws-empty">Select a session to view frames.</div>
          )}
        </section>

        <section className="ws-detail-pane">
          <h3>Frame detail</h3>
          {selectedFrame ? (
            <div className="ws-detail">
              <div>
                <strong>Time:</strong> {new Date(selectedFrame.captured_at).toLocaleString()}
              </div>
              <div>
                <strong>Direction:</strong> {selectedFrame.direction}
              </div>
              <div>
                <strong>Opcode:</strong> {selectedFrame.opcode}
                {selectedFrame.fin ? " (fin)" : " (continuation)"}
              </div>
              <div>
                <strong>Size:</strong> {formatBytes(selectedFrame.payload_size)}
              </div>
              {selectedFrame.injected ? <div><strong>Injected via replay</strong></div> : null}
              <pre className="ws-payload">
                {selectedFrame.text ?? `<binary — ${formatBytes(selectedFrame.payload_size)}>`}
              </pre>
            </div>
          ) : (
            <p className="muted">Select a frame above to see details.</p>
          )}

          <div className="ws-replay">
            <h3>Replay / inject frame</h3>
            <div className="ws-replay-row">
              <select
                value={replayDir}
                onChange={(e) => setReplayDir(e.target.value as WsDirection)}
              >
                <option value="client_to_server">Client → server</option>
                <option value="server_to_client">Server → client</option>
              </select>
              <select
                value={replayOpcode}
                onChange={(e) => setReplayOpcode(e.target.value as WsOpcode)}
              >
                <option value="text">text</option>
                <option value="binary">binary</option>
                <option value="ping">ping</option>
                <option value="pong">pong</option>
              </select>
            </div>
            <textarea
              value={replayText}
              onChange={(e) => setReplayText(e.target.value)}
              placeholder={
                replayOpcode === "text"
                  ? "Payload (UTF-8 text)"
                  : "Payload not editable for this opcode in v1 — empty frame will be sent"
              }
              disabled={replayOpcode !== "text"}
              rows={3}
            />
            <button
              className="btn-primary"
              onClick={() => void submitReplay()}
              disabled={!selectedSession || !!selectedSession?.ended_at}
            >
              <Send size={14} /> Inject frame
            </button>
            {selectedSession?.ended_at ? (
              <p className="muted">Session has ended — cannot inject new frames.</p>
            ) : null}
          </div>
        </section>
      </div>
    </div>
  );
}

export default WebSocketsPage;
