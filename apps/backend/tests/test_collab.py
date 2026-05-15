"""Live collaboration room (Feature J) signalling tests.

We rely on FastAPI's WebSocket TestClient rather than spinning up a real
event loop, so these are deterministic and fast.
"""

from __future__ import annotations

import json

from fastapi.testclient import TestClient


def _recv(ws) -> dict[str, object]:
    return json.loads(ws.receive_text())


def test_join_broadcasts_presence(client: TestClient) -> None:
    with client.websocket_connect("/collab/room/room-alpha") as a:
        a.send_text(json.dumps({"peer_id": "p-a", "display_name": "Alice"}))
        msg_a = _recv(a)
        assert msg_a["type"] == "presence"
        assert msg_a["event"] == "join"
        assert any(p["peer_id"] == "p-a" for p in msg_a["peers"])

        with client.websocket_connect("/collab/room/room-alpha") as b:
            b.send_text(json.dumps({"peer_id": "p-b", "display_name": "Bob"}))
            # b sees: its own join broadcast (peers=[a,b]).
            msg_b = _recv(b)
            assert msg_b["event"] == "join"
            assert msg_b["peer"]["peer_id"] == "p-b"
            # a also sees b's join.
            msg_a_after = _recv(a)
            assert msg_a_after["event"] == "join"
            assert msg_a_after["peer"]["peer_id"] == "p-b"


def test_rebroadcasts_messages_to_other_peer(client: TestClient) -> None:
    with client.websocket_connect("/collab/room/room-beta") as a, \
            client.websocket_connect("/collab/room/room-beta") as b:
        a.send_text(json.dumps({"peer_id": "p-a", "display_name": "A"}))
        # a receives its own join.
        _recv(a)
        b.send_text(json.dumps({"peer_id": "p-b", "display_name": "B"}))
        # b receives its join, a receives b's join.
        _recv(b)
        _recv(a)

        a.send_text(json.dumps({"type": "cursor", "x": 10, "y": 20}))
        # Broadcast goes to every peer (including a). Drain a's echo first
        # so b's receive is unambiguous.
        a_echo = _recv(a)
        assert a_echo["type"] == "cursor"
        msg_b = _recv(b)
        assert msg_b["type"] == "cursor"
        assert msg_b["from"] == "p-a"
        assert msg_b["x"] == 10


def test_rooms_endpoint_lists_active_room(client: TestClient) -> None:
    with client.websocket_connect("/collab/room/room-gamma") as a:
        a.send_text(json.dumps({"peer_id": "p-only", "display_name": "Solo"}))
        _recv(a)
        listing = client.get("/collab/rooms").json()
        ids = [r["id"] for r in listing["rooms"]]
        assert "room-gamma" in ids
