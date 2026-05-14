"""HTTP-only Collaborator-style out-of-band server.

A NyxProxy testing helper inspired by Burp's Collaborator. A pentester
creates a *session* (`POST /collaborator/sessions`) and is given a random,
URL-safe `session_id`. They can then embed callback URLs of the form
`https://<backend-host>/collaborator/c/<session_id>` (optionally with any
suffix path/query) into payloads. Every HTTP request that touches that
prefix is recorded into the session's ring buffer, exposed back via
`GET /collaborator/sessions/{id}/pings`.

Limitations:

- HTTP only. Real Collaborator also covers DNS + SMTP — both require
  privileged listeners and a dedicated DNS zone. A future phase will add
  these as separate processes.
- Pings are kept in-process. A multi-worker deployment would need a shared
  store (Redis, Postgres) which is out of scope for the AI gateway.
- Sessions never expire by themselves but a hard cap (`MAX_SESSIONS`)
  evicts the oldest one when full.
"""

from __future__ import annotations

import contextlib
import os
import secrets
import time
from collections import OrderedDict, deque

from fastapi import APIRouter, HTTPException, Request
from pydantic import BaseModel, Field

router = APIRouter()

MAX_SESSIONS = 128
MAX_PINGS_PER_SESSION = 200


class CollaboratorPing(BaseModel):
    timestamp: float
    method: str
    path: str
    query: str
    remote_addr: str | None
    headers: dict[str, str]
    body_preview: str
    body_size: int


class CollaboratorSession(BaseModel):
    session_id: str
    created_at: float
    polling_url: str
    pings: list[CollaboratorPing] = Field(default_factory=list)


class _SessionState:
    def __init__(self) -> None:
        self.created_at = time.time()
        self.pings: deque[CollaboratorPing] = deque(maxlen=MAX_PINGS_PER_SESSION)


_SESSIONS: OrderedDict[str, _SessionState] = OrderedDict()


def _new_session_id() -> str:
    return secrets.token_urlsafe(12)


def _register(session_id: str) -> _SessionState:
    if session_id in _SESSIONS:
        # Move-to-end keeps the LRU eviction policy honest.
        _SESSIONS.move_to_end(session_id)
        return _SESSIONS[session_id]
    state = _SessionState()
    _SESSIONS[session_id] = state
    while len(_SESSIONS) > MAX_SESSIONS:
        _SESSIONS.popitem(last=False)
    return state


@router.post("/collaborator/sessions", response_model=CollaboratorSession)
async def create_session(request: Request) -> CollaboratorSession:
    session_id = _new_session_id()
    state = _register(session_id)
    base = str(request.base_url).rstrip("/")
    return CollaboratorSession(
        session_id=session_id,
        created_at=state.created_at,
        polling_url=f"{base}/collaborator/c/{session_id}",
        pings=[],
    )


@router.get(
    "/collaborator/sessions/{session_id}/pings",
    response_model=list[CollaboratorPing],
)
async def list_pings(session_id: str) -> list[CollaboratorPing]:
    state = _SESSIONS.get(session_id)
    if state is None:
        raise HTTPException(status_code=404, detail="unknown session")
    return list(state.pings)


@router.api_route(
    "/collaborator/c/{session_id}",
    methods=["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"],
    include_in_schema=False,
)
@router.api_route(
    "/collaborator/c/{session_id}/{tail:path}",
    methods=["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"],
    include_in_schema=False,
)
async def collaborator_callback(
    request: Request, session_id: str, tail: str = ""
) -> dict[str, str]:
    state = _register(session_id)
    body = await request.body()
    preview = body[:256].decode("utf-8", errors="replace") if body else ""
    headers = {k: v for k, v in request.headers.items()}
    forwarded = request.headers.get("x-forwarded-for")
    remote_addr = (
        forwarded.split(",", 1)[0].strip()
        if forwarded
        else (request.client.host if request.client else None)
    )
    ping = CollaboratorPing(
        timestamp=time.time(),
        method=request.method,
        path="/" + tail if tail else "/",
        query=str(request.url.query),
        remote_addr=remote_addr,
        headers=headers,
        body_preview=preview,
        body_size=len(body),
    )
    state.pings.append(ping)
    return {"recorded": "ok"}


__all__ = ["CollaboratorPing", "CollaboratorSession", "router"]


def _clear_state_for_tests() -> None:
    """Reset module-level state — used by the test suite only."""
    _SESSIONS.clear()


def _force_max_sessions(value: int) -> None:
    """Test helper to lower the cap so the eviction test runs fast."""
    global MAX_SESSIONS
    MAX_SESSIONS = value
    while len(_SESSIONS) > MAX_SESSIONS:
        _SESSIONS.popitem(last=False)


# Allow tests to discover the env override path used in production
if env := os.environ.get("NYX_COLLAB_MAX_SESSIONS"):
    with contextlib.suppress(ValueError):
        MAX_SESSIONS = max(1, int(env))
