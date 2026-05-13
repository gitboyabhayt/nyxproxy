"""High-level analysis endpoints used by the NyxProxy desktop app.

Each endpoint wraps a curated prompt around the underlying chat provider so
the desktop app does not have to embed prompt strings.
"""

from __future__ import annotations

from fastapi import APIRouter, HTTPException, Request

from ..prompts import build_explain_prompt, build_payloads_prompt, build_vulns_prompt
from ..providers import ProviderError
from ..schemas import AnalyzeRequestBody, AnalyzeResponse, ChatMessage, PayloadRequestBody

router = APIRouter(prefix="/v1/analyze", tags=["analyze"])


def _provider(request: Request, name: str | None) -> tuple[str, object]:
    state = request.app.state
    provider_name = name or state.settings.default_provider
    provider = state.providers.get(provider_name)
    if provider is None:
        raise HTTPException(status_code=404, detail=f"unknown provider '{provider_name}'")
    return provider_name, provider


async def _run(request: Request, name: str | None, model: str | None, prompt: list[dict[str, str]]) -> AnalyzeResponse:
    provider_name, provider = _provider(request, name)
    messages = [ChatMessage(role=m["role"], content=m["content"]) for m in prompt]  # type: ignore[arg-type]
    try:
        result = await provider.chat(  # type: ignore[union-attr]
            client=request.app.state.http_client,
            messages=messages,
            model=model,
            temperature=0.2,
            max_tokens=1500,
        )
    except ProviderError as exc:
        raise HTTPException(status_code=exc.status_code, detail=str(exc)) from exc
    return AnalyzeResponse(
        provider=provider_name,
        model=result.model,
        content=result.choices[0].message.content,
    )


@router.post("/request", response_model=AnalyzeResponse)
async def explain_request(body: AnalyzeRequestBody, request: Request) -> AnalyzeResponse:
    return await _run(
        request, body.provider, body.model, build_explain_prompt(body.request, body.response)
    )


@router.post("/vulns", response_model=AnalyzeResponse)
async def find_vulns(body: AnalyzeRequestBody, request: Request) -> AnalyzeResponse:
    return await _run(
        request, body.provider, body.model, build_vulns_prompt(body.request, body.response)
    )


@router.post("/payloads", response_model=AnalyzeResponse)
async def generate_payloads(body: PayloadRequestBody, request: Request) -> AnalyzeResponse:
    prompt = build_payloads_prompt(body.request, body.parameter, body.attack_type, body.count)
    return await _run(request, body.provider, body.model, prompt)
