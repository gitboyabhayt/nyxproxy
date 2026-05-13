"""FastAPI entrypoint for the NyxProxy AI gateway."""

from __future__ import annotations

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

import httpx
from fastapi import Depends, FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from fastapi.security import HTTPAuthorizationCredentials, HTTPBearer

from . import __version__
from .config import Settings, get_settings
from .providers import build_providers
from .routes import analyze, chat, collaborator, health, providers

_BEARER_DEP = HTTPBearer(auto_error=False)
_BEARER = Depends(_BEARER_DEP)
_SETTINGS_DEP = Depends(get_settings)


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    settings = get_settings()
    app.state.settings = settings
    app.state.providers = build_providers(settings)
    app.state.http_client = httpx.AsyncClient(
        timeout=httpx.Timeout(settings.request_timeout_seconds),
        follow_redirects=True,
    )
    try:
        yield
    finally:
        await app.state.http_client.aclose()


def _enforce_token(
    credentials: HTTPAuthorizationCredentials | None = _BEARER,
    settings: Settings = _SETTINGS_DEP,
) -> None:
    if not settings.api_token:
        return
    if credentials is None or credentials.scheme.lower() != "bearer":
        raise HTTPException(status_code=401, detail="missing bearer token")
    if credentials.credentials != settings.api_token:
        raise HTTPException(status_code=403, detail="invalid token")


def create_app() -> FastAPI:
    settings = get_settings()
    app = FastAPI(
        title="NyxProxy AI Gateway",
        version=__version__,
        lifespan=lifespan,
    )
    app.add_middleware(
        CORSMiddleware,
        allow_origins=settings.origins_list(),
        allow_methods=["*"],
        allow_headers=["*"],
        allow_credentials=False,
    )
    app.include_router(health.router)
    app.include_router(providers.router, dependencies=[Depends(_enforce_token)])
    app.include_router(chat.router, dependencies=[Depends(_enforce_token)])
    app.include_router(analyze.router, dependencies=[Depends(_enforce_token)])
    # Collaborator endpoints are unauthenticated by design — attacker-controlled
    # callbacks need to be able to hit them. Session IDs act as the secret.
    app.include_router(collaborator.router)

    @app.exception_handler(Exception)
    async def unhandled(request: Request, exc: Exception) -> JSONResponse:
        return JSONResponse(
            status_code=500,
            content={"error": "internal_server_error", "detail": str(exc)},
        )

    return app


app = create_app()
