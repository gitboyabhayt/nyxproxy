"""Distributed scanning fleet (Feature K) tests."""

from __future__ import annotations

from pathlib import Path

import pytest
from fastapi.testclient import TestClient


@pytest.fixture(autouse=True)
def _isolate_db(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    db_path = tmp_path / "scan_jobs.db"
    monkeypatch.setenv("NYXPROXY_SCAN_JOBS_DB", str(db_path))
    # Reload the module's cached default
    import importlib

    from nyxproxy_backend.routes import scan_jobs

    importlib.reload(scan_jobs)
    yield


def test_enqueue_then_list(client: TestClient) -> None:
    payload = {
        "jobs": [
            {
                "target": {"url": "https://example.com/a", "method": "GET", "headers": {}},
                "rules": [],
                "label": "smoke-1",
            },
            {
                "target": {"url": "https://example.com/b", "method": "POST", "headers": {}},
                "rules": ["sqli"],
            },
        ]
    }
    r = client.post("/scan/jobs", json=payload)
    assert r.status_code == 200
    body = r.json()
    assert len(body["ids"]) == 2

    listing = client.get("/scan/jobs").json()
    assert len(listing) == 2
    assert all(j["status"] == "queued" for j in listing)


def test_next_claims_atomically_and_result_marks_done(client: TestClient) -> None:
    payload = {
        "jobs": [
            {
                "target": {"url": "https://example.com/a", "method": "GET", "headers": {}},
                "rules": [],
            }
        ]
    }
    enqueue = client.post("/scan/jobs", json=payload).json()
    job_id = enqueue["ids"][0]

    # First worker grabs it.
    claimed = client.get("/scan/jobs/next", params={"worker_id": "w1", "wait": 0}).json()
    assert claimed is not None
    assert claimed["id"] == job_id
    assert claimed["status"] == "in_progress"
    assert claimed["worker_id"] == "w1"

    # Second worker should get nothing (the only job is in_progress).
    none_now = client.get("/scan/jobs/next", params={"worker_id": "w2", "wait": 0}).json()
    assert none_now is None

    # Worker submits a result.
    submit = client.post(
        f"/scan/jobs/{job_id}/result",
        json={
            "status": "done",
            "findings": [{"rule": "sqli", "severity": "high"}],
            "elapsed_ms": 42,
        },
    ).json()
    assert submit["status"] == "done"
    assert submit["result"]["findings"][0]["rule"] == "sqli"
    assert submit["completed_at"] is not None


def test_clear_jobs_by_status(client: TestClient) -> None:
    client.post(
        "/scan/jobs",
        json={
            "jobs": [
                {"target": {"url": "https://x/", "method": "GET", "headers": {}}, "rules": []},
                {"target": {"url": "https://y/", "method": "GET", "headers": {}}, "rules": []},
            ]
        },
    )
    # Mark one as done.
    job_id = client.get("/scan/jobs/next", params={"worker_id": "w", "wait": 0}).json()["id"]
    client.post(
        f"/scan/jobs/{job_id}/result",
        json={"status": "done", "findings": [], "elapsed_ms": 1},
    )
    res = client.delete("/scan/jobs", params={"status": "done"}).json()
    assert res["deleted"] == 1
    remaining = client.get("/scan/jobs").json()
    assert len(remaining) == 1
    assert remaining[0]["status"] in {"queued", "in_progress"}
