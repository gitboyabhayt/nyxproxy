# WebSocket viewer & replay (Feature A)

NyxProxy captures live WebSocket traffic that flows through its MITM proxy
and exposes it in the desktop app as a dedicated **WebSockets** page.
Captured frames can be inspected and re-injected back into the live session
in either direction — like Burp's WebSockets History + Repeater, but
collapsed into one panel.

## What gets captured

For every `Upgrade: websocket` request handled by the proxy, NyxProxy:

1. Records a `WsSession` (id, url, host, started_at, ended_at, close_code).
2. Streams a `WsFrame` event for every RFC 6455 frame in either direction.
   - `opcode` (`text` / `binary` / `ping` / `pong` / `close` / `continuation`)
   - `fin`, `masked` flags
   - Original payload (base64-encoded; UTF-8 decoded `text` field for
     text frames)
   - `direction` (`client_to_server` / `server_to_client`)
   - `captured_at` timestamp
3. Records `close_code` / `close_reason` when the connection ends.

All of this is held in-process by `nyxproxy_core::websocket::WsStore`, with
configurable per-session frame caps and a session eviction policy so
long-lived connections don't grow unbounded.

## Architecture

```
Client ──TLS──┐
              │           ┌──────────────────────────┐
              │           │ nyxproxy-core MITM proxy │
              ▼           │                          │
       sniff Upgrade ────▶│ serve_websocket_upgrade  │
                          │   ├─ 101 to client       │
                          │   └─ upstream TLS dial   │
                          │ bridge_websocket         │
                          │   ├─ read_frame loop ×2  │
                          │   └─ WsStore.record(...) │
                          │                          │
                          │ replay channel ────▶ inject
                          └────────────┬─────────────┘
                                       │ broadcast
                                       ▼
                       Tauri event   nyxproxy://websocket
                                       │
                                       ▼
                        React WebSockets page (live)
```

## Replay / inject

The page lets you craft a new frame and send it as if it came from
either side:

| Direction          | Effect                                                |
| ------------------ | ----------------------------------------------------- |
| `client → server`  | Sends an unmasked-by-our-mask frame to the upstream.  |
| `server → client`  | Sends a server-style (unmasked) frame to the client.  |

Supported opcodes for v1: `text`, `binary`, `ping`, `pong`. `close`
remains an out-of-band operation (the session can be terminated by the
real client).

Replayed frames are flagged `injected = true` in the frame stream so it's
obvious in the table when a frame originated from you rather than the
live socket.

## Tauri command surface

| Command             | Args                                                           | Returns          |
| ------------------- | -------------------------------------------------------------- | ---------------- |
| `ws_list_sessions`  | —                                                              | `WsSession[]`    |
| `ws_get_session`    | `{ id: string }`                                               | `WsSession?`     |
| `ws_frames`         | `{ sessionId: string }`                                        | `WsFrame[]`      |
| `ws_replay`         | `{ args: { sessionId, direction, opcode, payloadB64?, text? } }` | `void`         |

Live updates are emitted on the `nyxproxy://websocket` event with
shape `{ kind: "session_started" | "frame" | "session_ended", ... }`.

## Tests

11 unit tests in `nyxproxy_core::websocket::tests` cover:

- Opcode encoding/decoding round-trip
- Control vs. data classification
- 0/2/8-byte length boundaries (`encode_len_16_boundary`, `encode_len_64_boundary`)
- Masked → unmasked round-trip
- Session recording + per-session frame caps
- Oldest-session eviction
- Replay failure when session has ended
- Bidirectional `proxy_pump` relay
