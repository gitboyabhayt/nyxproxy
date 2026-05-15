"""Live multi-user collaboration room (Feature J).

A pentester running NyxProxy and a colleague running NyxProxy elsewhere
want to see each other's history in real-time, share scope updates,
publish in-progress repeater requests, and (optionally) see each other's
cursor / focus / current page.

This module is a minimal *signalling room* — every peer joins a room by
WebSocket, every message they publish is rebroadcast to the other peers
in the same room. We do **not** persist any state; the room exists only
as long as someone is connected.

Compared to a full WebRTC mesh, this gives us:

- Sub-50 ms latency over a hosted backend (Render, Fly, your own box).
- Zero TURN server / STUN traffic to worry about \u2014 the backend relays.
- Trivial in-process implementation that the rest of the stack already
  understands (FastAPI + asyncio).

Trade-off: bandwidth and latency scale with the backend rather than the
number of peers. For 10-person rooms this is fine; if you need 100+ go
get a real SFU.

Room IDs act as the room secret. Anyone with the ID can join, by design \u2014
that's how Burp's Collaborator works too. Use long IDs.

Protocol:

- Client connects to ``ws://backend/collab/room/{room_id}``.
- First message MUST be a ``join`` event with ``{ peer_id, display_name }``.
- Subsequent messages are echoed to every peer in the room (including the
  sender, so single-page apps can resolve their own state machine).
- We emit ``presence`` events to every peer whenever someone joins/leaves.
"""

from __future__ import annotations

import asyncio
import json
import time
from collections import OrderedDict

from fastapi import APIRouter, WebSocket, WebSocketDisconnect
from pydantic import BaseModel

router = APIRouter(prefix="/collab", tags=["collab"])


class Peer(BaseModel):
    peer_id: str
    display_name: str
    joined_at: float


class _Room:
    def __init__(self) -> None:
        self.peers: dict[str, tuple[Peer, WebSocket]] = {}
        self.lock = asyncio.Lock()

    async def join(self, peer: Peer, ws: WebSocket) -> None:
        async with self.lock:
            self.peers[peer.peer_id] = (peer, ws)
        await self.broadcast(
            {
                "type": "presence",
                "event": "join",
                "peer": peer.model_dump(),
                "peers": self.snapshot(),
            },
            exclude=None,
        )

    async def leave(self, peer_id: str) -> None:
        async with self.lock:
            self.peers.pop(peer_id, None)
        await self.broadcast(
            {"type": "presence", "event": "leave", "peer_id": peer_id, "peers": self.snapshot()},
            exclude=peer_id,
        )

    def snapshot(self) -> list[dict[str, object]]:
        return [p.model_dump() for p, _ in self.peers.values()]

    async def broadcast(self, payload: dict[str, object], exclude: str | None) -> None:
        dead: list[str] = []
        for pid, (_, ws) in self.peers.items():
            if exclude is not None and pid == exclude:
                continue
            try:
                await ws.send_text(json.dumps(payload))
            except Exception:
                dead.append(pid)
        if dead:
            async with self.lock:
                for pid in dead:
                    self.peers.pop(pid, None)


MAX_ROOMS = 256
_ROOMS: OrderedDict[str, _Room] = OrderedDict()
_ROOMS_LOCK = asyncio.Lock()


async def _get_room(room_id: str) -> _Room:
    async with _ROOMS_LOCK:
        if room_id in _ROOMS:
            _ROOMS.move_to_end(room_id)
            return _ROOMS[room_id]
        room = _Room()
        _ROOMS[room_id] = room
        while len(_ROOMS) > MAX_ROOMS:
            _ROOMS.popitem(last=False)
        return room


@router.get("/rooms")
async def list_rooms() -> dict[str, object]:
    out = []
    for rid, room in _ROOMS.items():
        out.append({"id": rid, "peer_count": len(room.peers), "peers": room.snapshot()})
    return {"rooms": out}


@router.websocket("/room/{room_id}")
async def collab_room(ws: WebSocket, room_id: str) -> None:
    await ws.accept()
    room = await _get_room(room_id)

    # Expect a join handshake as the first message.
    try:
        first_raw = await asyncio.wait_for(ws.receive_text(), timeout=15.0)
    except (TimeoutError, WebSocketDisconnect):
        await ws.close(code=4000, reason="missing join")
        return
    try:
        first = json.loads(first_raw)
        peer = Peer(
            peer_id=str(first.get("peer_id") or "")[:64] or _short_id(),
            display_name=str(first.get("display_name") or "anonymous")[:64],
            joined_at=time.time(),
        )
    except Exception:
        await ws.close(code=4001, reason="invalid join payload")
        return
    await room.join(peer, ws)

    try:
        while True:
            raw = await ws.receive_text()
            try:
                payload = json.loads(raw)
            except json.JSONDecodeError:
                continue
            # Stamp every relayed message with the sender so the UI can
            # distinguish "self" vs "other peer" events.
            payload["from"] = peer.peer_id
            payload["ts"] = time.time()
            await room.broadcast(payload, exclude=None)
    except WebSocketDisconnect:
        pass
    except Exception:
        # Treat any unexpected error as a disconnect; the room cleans up
        # automatically.
        pass
    finally:
        await room.leave(peer.peer_id)


def _short_id() -> str:
    import secrets

    return secrets.token_urlsafe(8)
