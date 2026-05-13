"""OpenRouter provider — single API for many free + paid models."""

from __future__ import annotations

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, call_openai_compatible


class OpenRouterProvider(Provider):
    name = "openrouter"
    description = "OpenRouter — aggregator across many free and paid models."
    default_model = "meta-llama/llama-3.3-70b-instruct:free"
    endpoint = "https://openrouter.ai/api/v1/chat/completions"

    async def chat(
        self,
        *,
        client: httpx.AsyncClient,
        messages: list[ChatMessage],
        model: str | None,
        temperature: float,
        max_tokens: int,
    ) -> ChatResponse:
        self._require_key()
        return await call_openai_compatible(
            client=client,
            provider_name=self.name,
            url=self.endpoint,
            headers={
                "Authorization": f"Bearer {self.api_key}",
                "Content-Type": "application/json",
                "HTTP-Referer": "https://github.com/gitboyabhayt/nyxproxy",
                "X-Title": "NyxProxy",
            },
            messages=messages,
            model=model or self.default_model,
            temperature=temperature,
            max_tokens=max_tokens,
        )
