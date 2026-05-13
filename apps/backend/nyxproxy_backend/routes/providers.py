"""Provider discovery endpoint — the desktop app calls this on startup."""

from __future__ import annotations

from fastapi import APIRouter, Request

from ..schemas import ProvidersResponse

router = APIRouter(prefix="/v1", tags=["providers"])


@router.get("/providers", response_model=ProvidersResponse)
def list_providers(request: Request) -> ProvidersResponse:
    state = request.app.state
    return ProvidersResponse(
        default=state.settings.default_provider,
        providers=[p.info() for p in state.providers.values()],
    )
