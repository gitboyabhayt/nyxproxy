"""NVIDIA NIM (build.nvidia.com) — OpenAI-compatible, free tier."""

from __future__ import annotations

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, call_openai_compatible


class NvidiaProvider(Provider):
    name = "nvidia"
    description = "NVIDIA NIM — hosted Llama/Mixtral/Nemotron with a generous free tier."
    default_model = "meta/llama-3.3-70b-instruct"
    endpoint = "https://integrate.api.nvidia.com/v1/chat/completions"

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
