"""``/v1/chat/completions`` — generic chat passthrough."""

from __future__ import annotations

from fastapi import APIRouter, HTTPException, Request

from ..providers import ProviderError
from ..schemas import ChatRequest, ChatResponse

router = APIRouter(prefix="/v1", tags=["chat"])


@router.post("/chat/completions", response_model=ChatResponse)
async def chat_completions(payload: ChatRequest, request: Request) -> ChatResponse:
    state = request.app.state
    provider_name = payload.provider or state.settings.default_provider
    provider = state.providers.get(provider_name)
    if provider is None:
        raise HTTPException(status_code=404, detail=f"unknown provider '{provider_name}'")
    try:
        return await provider.chat(
            client=state.http_client,
            messages=payload.messages,
            model=payload.model,
            temperature=payload.temperature,
            max_tokens=payload.max_tokens,
        )
    except ProviderError as exc:
        raise HTTPException(status_code=exc.status_code, detail=str(exc)) from exc
