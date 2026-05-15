"""Distributed scanning fleet (Feature K).

A single laptop can only burn so much CPU on active scanning. Burp Pro
has a "Burp Suite Enterprise" upsell to fan scans out to a worker cluster;
NyxProxy ships the same capability for free.

This module exposes a job queue: clients publish a list of HTTP target
specs, workers long-poll for jobs, run the scan locally with the
existing ``nyxproxy-core`` scanner, and POST the findings back. The job
queue is persisted to a SQLite file under the data dir, so a backend
restart doesn't lose in-flight work.

Endpoints
---------

- ``POST /scan/jobs``           Enqueue a list of jobs. Returns IDs.
- ``GET  /scan/jobs``           List all known jobs with their current status.
- ``GET  /scan/jobs/next?worker_id=X``
                                Long-poll for the next ``queued`` job. Atomically
                                marks it ``in_progress``.
- ``POST /scan/jobs/{id}/result``
                                Worker uploads findings + final status.
- ``GET  /scan/jobs/{id}``      Fetch one job (status + result, if any).

Auth is the standard bearer token: workers are identified by an opaque
``worker_id`` string they choose at startup. We don't enforce uniqueness
\u2014 that's purely informational.
"""

from __future__ import annotations

import asyncio
import json
import os
import sqlite3
import time
import uuid
from contextlib import contextmanager
from typing import Any

from fastapi import APIRouter, HTTPException, Query
from pydantic import BaseModel, Field

router = APIRouter(prefix="/scan", tags=["scan"])

_DEFAULT_DB = os.getenv("NYXPROXY_SCAN_JOBS_DB", "scan_jobs.db")


class TargetSpec(BaseModel):
    """A single target to scan. Workers translate this into ``nyxproxy-core``
    scanner inputs locally."""

    url: str
    method: str = "GET"
    headers: dict[str, str] = Field(default_factory=dict)
    body_b64: str | None = None


class ScanJobIn(BaseModel):
    target: TargetSpec
    rules: list[str] = Field(default_factory=list, description="Optional rule filter")
    label: str | None = None


class ScanJob(BaseModel):
    id: str
    target: TargetSpec
    rules: list[str]
    label: str | None
    status: str  # queued | in_progress | done | failed
    worker_id: str | None
    created_at: float
    started_at: float | None
    completed_at: float | None
    result: dict[str, Any] | None


class EnqueueRequest(BaseModel):
    jobs: list[ScanJobIn]


class EnqueueResponse(BaseModel):
    ids: list[str]


class ScanResult(BaseModel):
    status: str  # done | failed
    findings: list[dict[str, Any]] = Field(default_factory=list)
    error: str | None = None
    elapsed_ms: int = 0


@contextmanager
def _conn():
    db_path = _DEFAULT_DB
    conn = sqlite3.connect(db_path)
    try:
        conn.execute(
            """
            CREATE TABLE IF NOT EXISTS scan_jobs (
              id TEXT PRIMARY KEY,
              target TEXT NOT NULL,
              rules TEXT NOT NULL,
              label TEXT,
              status TEXT NOT NULL,
              worker_id TEXT,
              created_at REAL NOT NULL,
              started_at REAL,
              completed_at REAL,
              result TEXT
            )
            """
        )
        conn.execute("CREATE INDEX IF NOT EXISTS scan_jobs_status_idx ON scan_jobs(status)")
        conn.commit()
        yield conn
    finally:
        conn.close()


def _row_to_job(row: tuple[Any, ...]) -> ScanJob:
    (
        id_,
        target_json,
        rules_json,
        label,
        status,
        worker_id,
        created_at,
        started_at,
        completed_at,
        result_json,
    ) = row
    return ScanJob(
        id=id_,
        target=TargetSpec.model_validate_json(target_json),
        rules=json.loads(rules_json),
        label=label,
        status=status,
        worker_id=worker_id,
        created_at=created_at,
        started_at=started_at,
        completed_at=completed_at,
        result=json.loads(result_json) if result_json else None,
    )


@router.post("/jobs", response_model=EnqueueResponse)
async def enqueue(body: EnqueueRequest) -> EnqueueResponse:
    if not body.jobs:
        raise HTTPException(status_code=400, detail="no jobs supplied")
    ids: list[str] = []
    now = time.time()
    with _conn() as conn:
        for job in body.jobs:
            job_id = uuid.uuid4().hex
            ids.append(job_id)
            conn.execute(
                "INSERT INTO scan_jobs (id, target, rules, label, status, created_at) "
                "VALUES (?, ?, ?, ?, 'queued', ?)",
                (
                    job_id,
                    job.target.model_dump_json(),
                    json.dumps(job.rules),
                    job.label,
                    now,
                ),
            )
        conn.commit()
    return EnqueueResponse(ids=ids)


@router.get("/jobs", response_model=list[ScanJob])
async def list_jobs(status: str | None = None) -> list[ScanJob]:
    with _conn() as conn:
        if status:
            rows = conn.execute(
                "SELECT * FROM scan_jobs WHERE status = ? ORDER BY created_at DESC",
                (status,),
            ).fetchall()
        else:
            rows = conn.execute(
                "SELECT * FROM scan_jobs ORDER BY created_at DESC LIMIT 500"
            ).fetchall()
    return [_row_to_job(r) for r in rows]


@router.get("/jobs/next", response_model=ScanJob | None)
async def next_job(
    worker_id: str = Query(..., min_length=1, max_length=64),
    wait: float = Query(15.0, ge=0.0, le=60.0),
) -> ScanJob | None:
    """Long-poll for the next queued job, atomically claim it."""

    deadline = time.time() + wait
    while True:
        with _conn() as conn:
            row = conn.execute(
                "SELECT * FROM scan_jobs WHERE status = 'queued' ORDER BY created_at LIMIT 1"
            ).fetchone()
            if row is not None:
                job_id = row[0]
                # Atomically claim by re-checking status.
                cur = conn.execute(
                    "UPDATE scan_jobs SET status = 'in_progress', worker_id = ?, "
                    "started_at = ? WHERE id = ? AND status = 'queued'",
                    (worker_id, time.time(), job_id),
                )
                conn.commit()
                if cur.rowcount == 1:
                    row = conn.execute("SELECT * FROM scan_jobs WHERE id = ?", (job_id,)).fetchone()
                    return _row_to_job(row)
                # someone else won the race \u2014 loop and try again.
                continue
        if time.time() >= deadline:
            return None
        await asyncio.sleep(0.5)


@router.get("/jobs/{job_id}", response_model=ScanJob)
async def get_job(job_id: str) -> ScanJob:
    with _conn() as conn:
        row = conn.execute("SELECT * FROM scan_jobs WHERE id = ?", (job_id,)).fetchone()
    if row is None:
        raise HTTPException(status_code=404, detail="job not found")
    return _row_to_job(row)


@router.post("/jobs/{job_id}/result", response_model=ScanJob)
async def submit_result(job_id: str, body: ScanResult) -> ScanJob:
    if body.status not in {"done", "failed"}:
        raise HTTPException(status_code=400, detail="status must be done or failed")
    with _conn() as conn:
        cur = conn.execute(
            "UPDATE scan_jobs SET status = ?, completed_at = ?, result = ? WHERE id = ?",
            (
                body.status,
                time.time(),
                json.dumps(
                    {
                        "findings": body.findings,
                        "error": body.error,
                        "elapsed_ms": body.elapsed_ms,
                    }
                ),
                job_id,
            ),
        )
        conn.commit()
        if cur.rowcount == 0:
            raise HTTPException(status_code=404, detail="job not found")
        row = conn.execute("SELECT * FROM scan_jobs WHERE id = ?", (job_id,)).fetchone()
    return _row_to_job(row)


@router.delete("/jobs")
async def clear_jobs(status: str | None = None) -> dict[str, Any]:
    """Operator helper: clear completed/failed jobs (or all). Required for tests
    and for a /scan/jobs UI to offer a 'clear history' button."""

    with _conn() as conn:
        if status:
            cur = conn.execute("DELETE FROM scan_jobs WHERE status = ?", (status,))
        else:
            cur = conn.execute("DELETE FROM scan_jobs")
        conn.commit()
        return {"deleted": cur.rowcount}
