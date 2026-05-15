# Live multi-user collaboration — Feature J

NyxProxy includes a **lightweight signalling backend** that lets several
teammates share a live room over a WebSocket. The implementation is
intentionally simple: it broadcasts every message a peer publishes to every
other peer in the same room and tracks presence (join / leave). There is no
persistence and no message replay — close the room and the state vanishes.

This is meant for *real-time coordination* — "hey, look at this finding" /
"about to fire the intruder, mute your scanner" / live cursor + selection
sharing — not for project history. Project history is what the .nyxproxy
file (and optionally cloud sync) is for.

## Architecture

```
+-------------+        WebSocket          +---------------+
|  Desktop A  | <-----------------------> |   Backend     |
+-------------+                           |               |
                                          | Room registry |
+-------------+        WebSocket          | (in-memory)   |
|  Desktop B  | <-----------------------> |               |
+-------------+                           +---------------+
```

* Endpoint: **`ws://<backend>/collab/room/{room_id}`**
* First frame from each peer must be `{"peer_id": "...", "display_name": "..."}`.
  The server stamps a `joined_at` epoch and broadcasts a `presence` event to
  the whole room.
* Every subsequent frame is JSON. The server adds `{"from": "<peer_id>", "ts":
  <epoch>}` to it and **rebroadcasts to every peer in the room, including the
  sender**. (Including the sender so the desktop UI can echo-confirm its own
  message without a separate "sent" model.)
* On disconnect the server fires a `presence` event with `event: "leave"`.

The room registry lives in-memory only (`OrderedDict` with LRU eviction after
256 rooms). This is deliberately a soft cap — if you need more, run multiple
backend instances behind a sticky session load-balancer.

## Endpoints

| Method | Path                          | Purpose                                |
| ------ | ----------------------------- | -------------------------------------- |
| WS     | `/collab/room/{room_id}`      | Join a room (first frame must be join) |
| GET    | `/collab/rooms`               | List currently-active rooms            |

`/collab/rooms` returns:

```json
{ "rooms": [{ "id": "room-alpha", "peer_count": 3, "peers": [...] }] }
```

It is unauthenticated by design — the room IDs are the access secret.

## Message shapes

A `presence` event:

```json
{
  "type": "presence",
  "event": "join",
  "peer": { "peer_id": "p-a", "display_name": "Alice", "joined_at": 1731234567.12 },
  "peers": [{ "peer_id": "p-a", "display_name": "Alice", ... }, ...]
}
```

A chat message after rebroadcast:

```json
{
  "type": "chat",
  "text": "hey is this XSS reflected?",
  "from": "p-a",
  "ts": 1731234580.45
}
```

A cursor / focus event (the desktop UI emits these whenever the Logger
selection changes):

```json
{ "type": "cursor", "x": 423, "y": 117, "panel": "logger", "from": "p-a", "ts": ... }
```

## Desktop UI

The **Live collab** page (`apps/desktop/src/pages/Collab.tsx`) provides:

* A list of currently-active rooms pulled from `GET /collab/rooms` every 5 s.
* "Join" / "Leave" controls with a per-machine peer ID stored in `localStorage`.
* Peer list (live, updates on every presence event).
* A chat pane (any `{"type": "chat", "text": ...}` message).
* A live-cursor / event log (last 10 `{"type": "cursor", ...}` messages).

The `CollabRoomClient` class in `apps/desktop/src/tauri/api.ts` is the only
wrapper code needed — joining publishes the handshake, and every push goes
out via the same socket.

## Security model

The collaboration room is a thin, **unauthenticated** signalling fabric. Anyone
who knows the room ID can join. Treat the room ID as an access secret and
share it over a trusted channel (Slack DM, Signal, etc.), or run the backend
inside your VPN.

For larger teams that need persistent rooms with per-user auth, the natural
next step is to put the WebSocket behind a reverse proxy that injects
`Authorization` headers and then add a `_authorize_room(room_id, peer_id)`
hook in `apps/backend/nyxproxy_backend/routes/collab.py`. That's out of scope
for the v1 feature.

## Tests

[`apps/backend/tests/test_collab.py`](../../apps/backend/tests/test_collab.py)
covers:

* Two peers joining the same room and seeing each other in presence events.
* A `chat` / `cursor` message from peer A reaching peer B with `"from":
  "p-a"` stamped on it.
* `GET /collab/rooms` listing the active room.

## Files

| File | Purpose |
| ---- | ------- |
| `apps/backend/nyxproxy_backend/routes/collab.py` | FastAPI WebSocket route, room registry |
| `apps/backend/tests/test_collab.py` | Signalling tests |
| `apps/desktop/src/tauri/api.ts` → `CollabRoomClient` / `CollabApi` | WS client + REST list |
| `apps/desktop/src/pages/Collab.tsx` | Live collab page UI |
