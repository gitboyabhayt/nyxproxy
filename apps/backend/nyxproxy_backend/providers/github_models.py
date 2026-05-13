"""GitHub Models — free, generous limits, OpenAI-compatible."""

from __future__ import annotations

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, call_openai_compatible


class GithubModelsProvider(Provider):
    name = "github_models"
    description = "GitHub Models — free access to OpenAI/Meta/Phi models, uses a GitHub PAT."
    default_model = "openai/gpt-4o-mini"
    endpoint = "https://models.github.ai/inference/chat/completions"

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
                "Accept": "application/vnd.github+json",
            },
            messages=messages,
            model=model or self.default_model,
            temperature=temperature,
            max_tokens=max_tokens,
        )
