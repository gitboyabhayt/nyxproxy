import { useEffect, useMemo, useRef, useState } from "react";
import { LogIn, LogOut, Send, Users } from "lucide-react";

import { DEFAULT_BACKEND_URL } from "@/lib/backend";
import { useAppStore } from "@/state/store";
import { CollabApi, CollabRoomClient, type CollabMessage, type CollabPeer } from "@/tauri/api";

interface RoomState {
  peers: CollabPeer[];
  messages: CollabMessage[];
}

function shortId(): string {
  return crypto.randomUUID().slice(0, 8);
}

export function CollabPage() {
  const toast = useAppStore((s) => s.toast);
  const settings = useAppStore((s) => s.settings);
  const backendUrl = settings?.backend_url || DEFAULT_BACKEND_URL;

  const [roomId, setRoomId] = useState<string>("");
  const [displayName, setDisplayName] = useState<string>(
    () => localStorage.getItem("nyx-collab-name") || "anonymous",
  );
  const [peerId] = useState<string>(
    () => localStorage.getItem("nyx-collab-peer") || (() => {
      const id = shortId();
      localStorage.setItem("nyx-collab-peer", id);
      return id;
    })(),
  );
  const [state, setState] = useState<RoomState>({ peers: [], messages: [] });
  const [connected, setConnected] = useState(false);
  const [chatDraft, setChatDraft] = useState("");
  const [knownRooms, setKnownRooms] = useState<{ id: string; peer_count: number }[]>(
    [],
  );
  const clientRef = useRef<CollabRoomClient | null>(null);

  const refreshRooms = async () => {
    try {
      const res = await CollabApi.listRooms(backendUrl);
      setKnownRooms(res.rooms.map((r) => ({ id: r.id, peer_count: r.peer_count })));
    } catch {
      // ignore — backend may be offline
    }
  };

  useEffect(() => {
    refreshRooms();
    const interval = setInterval(refreshRooms, 5000);
    return () => clearInterval(interval);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [backendUrl]);

  const join = async () => {
    if (!roomId.trim()) {
      toast("error", "Enter or pick a room ID first.");
      return;
    }
    if (clientRef.current) clientRef.current.close();
    const client = new CollabRoomClient(
      backendUrl,
      roomId.trim(),
      peerId,
      displayName.trim() || "anonymous",
    );
    client.on((msg) => {
      setState((s) => {
        if (msg.type === "presence") {
          return {
            peers: (msg.peers as CollabPeer[]) ?? s.peers,
            messages: s.messages,
          };
        }
        return { peers: s.peers, messages: [...s.messages.slice(-199), msg] };
      });
    });
    try {
      await client.connect();
      clientRef.current = client;
      setConnected(true);
      localStorage.setItem("nyx-collab-name", displayName);
      toast("info", `Joined room ${roomId.trim()}.`);
    } catch (err) {
      toast("error", `Join failed: ${err}`);
    }
  };

  const leave = () => {
    clientRef.current?.close();
    clientRef.current = null;
    setConnected(false);
    setState({ peers: [], messages: [] });
  };

  const sendChat = () => {
    if (!chatDraft.trim()) return;
    clientRef.current?.publish("chat", { text: chatDraft });
    setChatDraft("");
  };

  useEffect(() => {
    return () => {
      clientRef.current?.close();
    };
  }, []);

  const chatMessages = useMemo(
    () => state.messages.filter((m) => m.type === "chat"),
    [state.messages],
  );
  const cursorMessages = useMemo(
    () => state.messages.filter((m) => m.type === "cursor").slice(-10),
    [state.messages],
  );

  return (
    <>
      <div className="toolbar" style={{ gap: 8 }}>
        <input
          placeholder="Room ID"
          value={roomId}
          onChange={(e) => setRoomId(e.target.value)}
          disabled={connected}
          style={{ flex: "0 1 220px", minWidth: 160 }}
        />
        <input
          placeholder="Display name"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          disabled={connected}
          style={{ flex: "0 1 180px", minWidth: 120 }}
        />
        {!connected ? (
          <button className="btn primary" onClick={join}>
            <LogIn size={14} /> Join
          </button>
        ) : (
          <button className="btn danger" onClick={leave}>
            <LogOut size={14} /> Leave
          </button>
        )}
        <button
          className="btn ghost"
          onClick={() => setRoomId(`nyx-${shortId()}`)}
          disabled={connected}
        >
          New room ID
        </button>
        <span style={{ flex: 1 }} />
        <span className="muted" style={{ fontSize: 11 }}>
          Backend: {backendUrl}
        </span>
      </div>

      <div
        className="main-content"
        style={{ display: "flex", gap: 12, overflow: "hidden", flexWrap: "wrap" }}
      >
        <div
          className="panel"
          style={{ flex: "1 1 260px", minWidth: 240, maxWidth: 360, display: "flex", flexDirection: "column" }}
        >
          <div className="panel-header">
            <Users size={14} /> Peers ({state.peers.length})
          </div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            {!connected ? (
              <div className="muted" style={{ fontSize: 12 }}>
                Join a room to see live peers.
              </div>
            ) : state.peers.length === 0 ? (
              <div className="muted" style={{ fontSize: 12 }}>
                You're alone in this room.
              </div>
            ) : (
              state.peers.map((p) => (
                <div key={p.peer_id} className="nav-item" style={{ cursor: "default" }}>
                  <span className="badge badge-low">{p.peer_id === peerId ? "you" : "live"}</span>
                  <span>{p.display_name}</span>
                </div>
              ))
            )}
          </div>
          <div className="panel-header" style={{ borderTop: "1px solid var(--border)" }}>
            Active rooms
          </div>
          <div className="panel-body" style={{ overflow: "auto", maxHeight: 200 }}>
            {knownRooms.length === 0 ? (
              <div className="muted" style={{ fontSize: 12 }}>
                No active rooms.
              </div>
            ) : (
              knownRooms.map((r) => (
                <div
                  key={r.id}
                  className="nav-item"
                  onClick={() => !connected && setRoomId(r.id)}
                  style={{ cursor: connected ? "default" : "pointer" }}
                >
                  <span style={{ flex: 1, fontFamily: "var(--font-mono)" }}>{r.id}</span>
                  <span className="badge">{r.peer_count}</span>
                </div>
              ))
            )}
          </div>
        </div>

        <div className="panel" style={{ flex: "2 1 360px", minWidth: 280, display: "flex", flexDirection: "column" }}>
          <div className="panel-header">Chat</div>
          <div className="panel-body" style={{ overflow: "auto", flex: 1 }}>
            {chatMessages.length === 0 ? (
              <div className="muted" style={{ fontSize: 12 }}>
                No chat messages yet. Type below to broadcast to every peer in this room.
              </div>
            ) : (
              chatMessages.map((m, i) => (
                <div key={i} style={{ marginBottom: 8 }}>
                  <span style={{ fontWeight: 600 }}>
                    {state.peers.find((p) => p.peer_id === m.from)?.display_name ?? m.from}
                  </span>
                  <span className="muted" style={{ fontSize: 11, marginLeft: 8 }}>
                    {m.ts ? new Date((m.ts as number) * 1000).toLocaleTimeString() : ""}
                  </span>
                  <div style={{ fontSize: 13, marginTop: 2 }}>{String(m.text ?? "")}</div>
                </div>
              ))
            )}
          </div>
          <div className="row-wrap" style={{ padding: 8, borderTop: "1px solid var(--border)" }}>
            <input
              placeholder="Type a message…"
              value={chatDraft}
              onChange={(e) => setChatDraft(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && sendChat()}
              disabled={!connected}
              style={{ flex: 1, minWidth: 200 }}
            />
            <button className="btn primary" onClick={sendChat} disabled={!connected}>
              <Send size={14} /> Send
            </button>
          </div>
        </div>

        <div className="panel" style={{ flex: "1 1 240px", minWidth: 220, maxWidth: 320, display: "flex", flexDirection: "column" }}>
          <div className="panel-header">Live cursors / events</div>
          <div className="panel-body" style={{ overflow: "auto" }}>
            {cursorMessages.length === 0 ? (
              <div className="muted" style={{ fontSize: 12 }}>
                Peers publishing cursor / focus events will show up here.
              </div>
            ) : (
              cursorMessages.map((m, i) => (
                <div
                  key={i}
                  className="mono"
                  style={{ fontSize: 11, padding: "4px 8px" }}
                >
                  {m.from} → ({String(m.x ?? "?")}, {String(m.y ?? "?")})
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </>
  );
}
