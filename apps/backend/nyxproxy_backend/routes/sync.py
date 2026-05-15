"""Cloud sync via Supabase (Feature F).

A pentester running NyxProxy on a laptop wants their workspace (history,
findings, intercepted requests, plugin state) to live on at least one
other machine — for collaboration, for disaster recovery, and so they
can pick up an engagement on a different host the next day.

This route exposes a *thin* abstraction over a Supabase project that any
self-hoster can stand up in ~3 minutes:

1. Create a free Supabase project at <https://supabase.com>.
2. Create a single table called ``nyx_workspaces`` with the schema below.
3. Set the ``SUPABASE_URL`` and ``SUPABASE_SERVICE_KEY`` environment
   variables on the NyxProxy backend.

Schema (copy/paste into the SQL editor)::

    create table nyx_workspaces (
      id text primary key,
      owner text not null,
      revision bigint not null default 0,
      updated_at timestamptz not null default now(),
      payload jsonb not null
    );
    create index nyx_workspaces_owner_idx on nyx_workspaces (owner);

When the env vars are missing every endpoint returns ``503`` with the
machine-readable ``error: "feature_disabled"`` body. This is what the
desktop UI uses to hide the sync controls when the user is pointing at a
backend that hasn't been configured for sync.
"""

from __future__ import annotations

import os
import time
from typing import Any

import httpx
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

router = APIRouter(prefix="/sync", tags=["sync"])


class SyncWorkspace(BaseModel):
    id: str
    owner: str = Field(..., description="Stable user identifier (email, uuid, etc.)")
    revision: int = Field(0, ge=0)
    payload: dict[str, Any] = Field(default_factory=dict)


class SyncPushRequest(BaseModel):
    workspace: SyncWorkspace
    expected_revision: int | None = Field(
        default=None,
        description=(
            "If set, the push is rejected with 409 when the stored revision differs. "
            "Used by clients to detect concurrent edits."
        ),
    )


class SyncPushResponse(BaseModel):
    workspace: SyncWorkspace
    updated_at: str


class SyncPullResponse(BaseModel):
    workspace: SyncWorkspace | None
    updated_at: str | None


def _supabase_config() -> tuple[str, str] | None:
    url = os.getenv("SUPABASE_URL")
    key = os.getenv("SUPABASE_SERVICE_KEY") or os.getenv("SUPABASE_KEY")
    if not url or not key:
        return None
    return url.rstrip("/"), key


def _disabled() -> HTTPException:
    return HTTPException(
        status_code=503,
        detail={
            "error": "feature_disabled",
            "message": (
                "Cloud sync is not configured. Set SUPABASE_URL and "
                "SUPABASE_SERVICE_KEY on the backend, or run NyxProxy locally."
            ),
        },
    )


def _headers(key: str) -> dict[str, str]:
    return {
        "apikey": key,
        "Authorization": f"Bearer {key}",
        "Accept": "application/json",
        "Content-Type": "application/json",
    }


@router.get("/status")
async def sync_status() -> dict[str, Any]:
    """Cheap probe used by the UI to decide whether to render sync controls."""

    cfg = _supabase_config()
    return {
        "enabled": cfg is not None,
        "provider": "supabase" if cfg is not None else None,
    }


@router.get("/pull/{owner}/{workspace_id}", response_model=SyncPullResponse)
async def sync_pull(owner: str, workspace_id: str) -> SyncPullResponse:
    cfg = _supabase_config()
    if cfg is None:
        raise _disabled()
    url, key = cfg
    endpoint = (
        f"{url}/rest/v1/nyx_workspaces"
        f"?id=eq.{workspace_id}&owner=eq.{owner}&select=*"
    )
    async with httpx.AsyncClient(timeout=httpx.Timeout(15.0)) as client:
        res = await client.get(endpoint, headers=_headers(key))
    if res.status_code != 200:
        raise HTTPException(
            status_code=502,
            detail={"error": "supabase_error", "status": res.status_code, "body": res.text},
        )
    rows = res.json()
    if not rows:
        return SyncPullResponse(workspace=None, updated_at=None)
    row = rows[0]
    return SyncPullResponse(
        workspace=SyncWorkspace(
            id=row["id"],
            owner=row["owner"],
            revision=int(row.get("revision", 0)),
            payload=row.get("payload") or {},
        ),
        updated_at=row.get("updated_at"),
    )


@router.post("/push", response_model=SyncPushResponse)
async def sync_push(body: SyncPushRequest) -> SyncPushResponse:
    cfg = _supabase_config()
    if cfg is None:
        raise _disabled()
    url, key = cfg

    # Optimistic concurrency: if the caller asked us to gate on a specific
    # revision number we first pull and check.
    async with httpx.AsyncClient(timeout=httpx.Timeout(15.0)) as client:
        if body.expected_revision is not None:
            existing = await client.get(
                f"{url}/rest/v1/nyx_workspaces"
                f"?id=eq.{body.workspace.id}&owner=eq.{body.workspace.owner}&select=revision",
                headers=_headers(key),
            )
            if existing.status_code == 200:
                rows = existing.json()
                if rows:
                    stored = int(rows[0].get("revision", 0))
                    if stored != body.expected_revision:
                        raise HTTPException(
                            status_code=409,
                            detail={
                                "error": "revision_conflict",
                                "stored_revision": stored,
                                "expected_revision": body.expected_revision,
                            },
                        )

        # Upsert. PostgREST `on_conflict` resolves the primary key collision.
        upsert_payload = {
            "id": body.workspace.id,
            "owner": body.workspace.owner,
            "revision": body.workspace.revision,
            "payload": body.workspace.payload,
        }
        res = await client.post(
            f"{url}/rest/v1/nyx_workspaces?on_conflict=id",
            headers={**_headers(key), "Prefer": "return=representation,resolution=merge-duplicates"},
            json=upsert_payload,
        )
    if res.status_code not in (200, 201):
        raise HTTPException(
            status_code=502,
            detail={"error": "supabase_error", "status": res.status_code, "body": res.text},
        )
    rows = res.json()
    row = rows[0] if isinstance(rows, list) and rows else upsert_payload
    return SyncPushResponse(
        workspace=SyncWorkspace(
            id=row.get("id", body.workspace.id),
            owner=row.get("owner", body.workspace.owner),
            revision=int(row.get("revision", body.workspace.revision)),
            payload=row.get("payload") or body.workspace.payload,
        ),
        updated_at=row.get("updated_at") or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    )


@router.delete("/{owner}/{workspace_id}")
async def sync_delete(owner: str, workspace_id: str) -> dict[str, Any]:
    cfg = _supabase_config()
    if cfg is None:
        raise _disabled()
    url, key = cfg
    async with httpx.AsyncClient(timeout=httpx.Timeout(15.0)) as client:
        res = await client.delete(
            f"{url}/rest/v1/nyx_workspaces?id=eq.{workspace_id}&owner=eq.{owner}",
            headers=_headers(key),
        )
    if res.status_code not in (200, 204):
        raise HTTPException(
            status_code=502,
            detail={"error": "supabase_error", "status": res.status_code, "body": res.text},
        )
    return {"deleted": True}
