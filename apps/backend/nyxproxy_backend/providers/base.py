"""Base classes for AI providers.

Every provider exposes the same async ``chat`` interface: given a list of
OpenAI-style messages plus model/temperature/max_tokens, return a normalised
:class:`ChatResponse`. Differences in wire format are absorbed inside each
adapter.
"""

from __future__ import annotations

import abc

import httpx

from ..schemas import ChatChoice, ChatMessage, ChatResponse, ChatUsage, ProviderInfo


class ProviderError(RuntimeError):
    """Raised when a provider fails to handle a request."""

    def __init__(self, provider: str, message: str, *, status_code: int = 502) -> None:
        super().__init__(f"[{provider}] {message}")
        self.provider = provider
        self.status_code = status_code


class Provider(abc.ABC):
    name: str
    description: str
    default_model: str

    def __init__(self, *, api_key: str | None = None) -> None:
        self.api_key = api_key

    @property
    def available(self) -> bool:
        return bool(self.api_key)

    def info(self) -> ProviderInfo:
        return ProviderInfo(
            name=self.name,
            available=self.available,
            default_model=self.default_model,
            description=self.description,
        )

    def _require_key(self) -> None:
        if not self.api_key:
            raise ProviderError(
                self.name,
                "no API key configured on the backend (set the matching env var)",
                status_code=503,
            )

    @abc.abstractmethod
    async def chat(
        self,
        *,
        client: httpx.AsyncClient,
        messages: list[ChatMessage],
        model: str | None,
        temperature: float,
        max_tokens: int,
    ) -> ChatResponse: ...

    @staticmethod
    def _make_response(
        provider: str,
        model: str,
        content: str,
        usage: ChatUsage | None = None,
        finish_reason: str | None = None,
    ) -> ChatResponse:
        return ChatResponse(
            provider=provider,
            model=model,
            choices=[
                ChatChoice(
                    index=0,
                    message=ChatMessage(role="assistant", content=content),
                    finish_reason=finish_reason,
                )
            ],
            usage=usage,
        )


def _raise_for_http_error(provider: str, response: httpx.Response) -> None:
    if response.is_success:
        return
    snippet = response.text[:500].replace("\n", " ")
    raise ProviderError(
        provider,
        f"upstream returned HTTP {response.status_code}: {snippet}",
        status_code=502 if response.status_code >= 500 else 400,
    )


def openai_compatible_payload(
    messages: list[ChatMessage],
    model: str,
    temperature: float,
    max_tokens: int,
) -> dict[str, object]:
    return {
        "model": model,
        "messages": [m.model_dump() for m in messages],
        "temperature": temperature,
        "max_tokens": max_tokens,
    }


def parse_openai_chat_response(
    provider_name: str, model: str, body: dict[str, object]
) -> ChatResponse:
    choices = body.get("choices") or []
    if not choices:
        raise ProviderError(provider_name, "upstream returned no choices")
    first = choices[0]
    if not isinstance(first, dict):
        raise ProviderError(provider_name, "malformed choices entry")

    message = first.get("message") or {}
    if not isinstance(message, dict):
        raise ProviderError(provider_name, "malformed message entry")

    content = message.get("content")
    if not isinstance(content, str):
        if isinstance(content, list):  # some providers return list of parts
            text_parts = [p.get("text", "") for p in content if isinstance(p, dict)]
            content = "".join(text_parts)
        else:
            raise ProviderError(provider_name, "no string content in upstream response")

    finish_reason = first.get("finish_reason") if isinstance(first.get("finish_reason"), str) else None

    usage_payload = body.get("usage") if isinstance(body.get("usage"), dict) else None
    usage = None
    if usage_payload:
        usage = ChatUsage(
            prompt_tokens=usage_payload.get("prompt_tokens"),
            completion_tokens=usage_payload.get("completion_tokens"),
            total_tokens=usage_payload.get("total_tokens"),
        )

    returned_model = body.get("model") if isinstance(body.get("model"), str) else model

    return ChatResponse(
        provider=provider_name,
        model=returned_model,
        choices=[
            ChatChoice(
                index=0,
                message=ChatMessage(role="assistant", content=content),
                finish_reason=finish_reason,
            )
        ],
        usage=usage,
    )


async def call_openai_compatible(
    *,
    client: httpx.AsyncClient,
    provider_name: str,
    url: str,
    headers: dict[str, str],
    messages: list[ChatMessage],
    model: str,
    temperature: float,
    max_tokens: int,
) -> ChatResponse:
    payload = openai_compatible_payload(messages, model, temperature, max_tokens)
    response = await client.post(url, headers=headers, json=payload)
    _raise_for_http_error(provider_name, response)
    body = response.json()
    if not isinstance(body, dict):
        raise ProviderError(provider_name, "upstream JSON was not an object")
    return parse_openai_chat_response(provider_name, model, body)
