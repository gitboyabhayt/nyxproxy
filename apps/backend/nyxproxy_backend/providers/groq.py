"""Groq provider — OpenAI-compatible, very fast Llama-3.x and Mixtral models."""

from __future__ import annotations

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, call_openai_compatible


class GroqProvider(Provider):
    name = "groq"
    description = "Groq Cloud — ultra-fast inference for Llama 3.x and Mixtral."
    default_model = "llama-3.3-70b-versatile"
    endpoint = "https://api.groq.com/openai/v1/chat/completions"

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
            },
            messages=messages,
            model=model or self.default_model,
            temperature=temperature,
            max_tokens=max_tokens,
        )
