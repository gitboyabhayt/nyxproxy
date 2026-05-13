"""Health and root endpoints."""

from __future__ import annotations

from fastapi import APIRouter

from .. import __version__

router = APIRouter(tags=["meta"])


@router.get("/")
def root() -> dict[str, str]:
    return {
        "service": "nyxproxy-backend",
        "version": __version__,
        "docs": "/docs",
    }


@router.get("/healthz")
def healthz() -> dict[str, str]:
    return {"status": "ok"}
