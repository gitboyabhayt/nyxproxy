from __future__ import annotations

from fastapi.testclient import TestClient

from nyxproxy_backend.routes import collaborator as collaborator_module


def setup_function() -> None:
    collaborator_module._clear_state_for_tests()
    collaborator_module._force_max_sessions(128)


def test_create_session_returns_polling_url(client: TestClient) -> None:
    response = client.post("/collaborator/sessions")
    assert response.status_code == 200
    body = response.json()
    assert body["session_id"]
    assert body["polling_url"].endswith(f"/collaborator/c/{body['session_id']}")
    assert body["pings"] == []


def test_callback_is_recorded_into_session(client: TestClient) -> None:
    response = client.post("/collaborator/sessions")
    session_id = response.json()["session_id"]

    # Simulate three pings from "attacker" payload callbacks.
    client.get(f"/collaborator/c/{session_id}")
    client.post(
        f"/collaborator/c/{session_id}/inner",
        params={"who": "ssrf"},
        content=b"leaked-token",
    )
    client.put(
        f"/collaborator/c/{session_id}/some/deep/path",
        headers={"X-Forwarded-For": "10.20.30.40"},
    )

    pings = client.get(f"/collaborator/sessions/{session_id}/pings").json()
    assert len(pings) == 3
    methods = [p["method"] for p in pings]
    assert methods == ["GET", "POST", "PUT"]
    assert pings[1]["query"] == "who=ssrf"
    assert pings[1]["body_preview"] == "leaked-token"
    assert pings[2]["remote_addr"] == "10.20.30.40"


def test_unknown_session_returns_404(client: TestClient) -> None:
    response = client.get("/collaborator/sessions/does-not-exist/pings")
    assert response.status_code == 404


def test_pings_ring_buffer_caps_at_limit(client: TestClient) -> None:
    response = client.post("/collaborator/sessions")
    session_id = response.json()["session_id"]
    # Push more than the buffer cap and ensure it caps.
    for _ in range(collaborator_module.MAX_PINGS_PER_SESSION + 25):
        client.get(f"/collaborator/c/{session_id}")
    pings = client.get(f"/collaborator/sessions/{session_id}/pings").json()
    assert len(pings) == collaborator_module.MAX_PINGS_PER_SESSION


def test_sessions_evict_when_cap_reached() -> None:
    # Drive the eviction logic directly on the module — exercising it via the
    # router would require N HTTP calls; this is faster and identical.
    collaborator_module._force_max_sessions(3)
    collaborator_module._register("a")
    collaborator_module._register("b")
    collaborator_module._register("c")
    collaborator_module._register("d")
    assert "a" not in collaborator_module._SESSIONS
    assert {"b", "c", "d"} == set(collaborator_module._SESSIONS.keys())
