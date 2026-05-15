"""Cloud sync (Supabase) route tests."""

from __future__ import annotations

import httpx
import pytest
import respx
from fastapi.testclient import TestClient


def test_status_disabled_when_env_missing(
    client: TestClient, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.delenv("SUPABASE_URL", raising=False)
    monkeypatch.delenv("SUPABASE_KEY", raising=False)
    monkeypatch.delenv("SUPABASE_SERVICE_KEY", raising=False)
    r = client.get("/sync/status")
    assert r.status_code == 200
    assert r.json() == {"enabled": False, "provider": None}


def test_push_returns_503_when_env_missing(
    client: TestClient, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.delenv("SUPABASE_URL", raising=False)
    monkeypatch.delenv("SUPABASE_KEY", raising=False)
    monkeypatch.delenv("SUPABASE_SERVICE_KEY", raising=False)
    payload = {"workspace": {"id": "w1", "owner": "u@example.com", "revision": 0, "payload": {}}}
    r = client.post("/sync/push", json=payload)
    assert r.status_code == 503
    assert r.json()["detail"]["error"] == "feature_disabled"


def test_status_enabled_with_env(client: TestClient, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("SUPABASE_URL", "https://demo.supabase.co")
    monkeypatch.setenv("SUPABASE_SERVICE_KEY", "anon-key")
    r = client.get("/sync/status")
    assert r.status_code == 200
    assert r.json() == {"enabled": True, "provider": "supabase"}


@respx.mock
def test_push_then_pull_round_trip(client: TestClient, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("SUPABASE_URL", "https://demo.supabase.co")
    monkeypatch.setenv("SUPABASE_SERVICE_KEY", "anon-key")

    upsert_route = respx.post("https://demo.supabase.co/rest/v1/nyx_workspaces").mock(
        return_value=httpx.Response(
            201,
            json=[
                {
                    "id": "w1",
                    "owner": "u@example.com",
                    "revision": 1,
                    "payload": {"hi": True},
                    "updated_at": "2026-05-14T16:00:00Z",
                }
            ],
        )
    )
    pull_route = respx.get("https://demo.supabase.co/rest/v1/nyx_workspaces").mock(
        return_value=httpx.Response(
            200,
            json=[
                {
                    "id": "w1",
                    "owner": "u@example.com",
                    "revision": 1,
                    "payload": {"hi": True},
                    "updated_at": "2026-05-14T16:00:00Z",
                }
            ],
        )
    )

    push = client.post(
        "/sync/push",
        json={
            "workspace": {
                "id": "w1",
                "owner": "u@example.com",
                "revision": 1,
                "payload": {"hi": True},
            },
        },
    )
    assert push.status_code == 200, push.text
    assert push.json()["workspace"]["revision"] == 1
    assert upsert_route.called

    pull = client.get("/sync/pull/u@example.com/w1")
    assert pull.status_code == 200, pull.text
    body = pull.json()
    assert body["workspace"]["payload"] == {"hi": True}
    assert pull_route.called


@respx.mock
def test_push_conflict_when_expected_revision_mismatches(
    client: TestClient, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("SUPABASE_URL", "https://demo.supabase.co")
    monkeypatch.setenv("SUPABASE_SERVICE_KEY", "anon-key")

    respx.get("https://demo.supabase.co/rest/v1/nyx_workspaces").mock(
        return_value=httpx.Response(200, json=[{"revision": 5}])
    )

    r = client.post(
        "/sync/push",
        json={
            "workspace": {
                "id": "w1",
                "owner": "u@example.com",
                "revision": 6,
                "payload": {},
            },
            "expected_revision": 4,
        },
    )
    assert r.status_code == 409
    detail = r.json()["detail"]
    assert detail["error"] == "revision_conflict"
    assert detail["stored_revision"] == 5
