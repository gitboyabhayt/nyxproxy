"""Ollama provider — talks to a locally running ``ollama serve`` instance."""

from __future__ import annotations

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, ProviderError, _raise_for_http_error


class OllamaProvider(Provider):
    name = "ollama"
    description = "Ollama (local) — run open-source models on your own machine."
    default_model = "llama3.2"

    def __init__(self, *, base_url: str = "http://localhost:11434") -> None:
        super().__init__(api_key=None)
        self.base_url = base_url.rstrip("/")

    @property
    def available(self) -> bool:
        # Always advertised — availability is best-effort against a local daemon.
        return True

    async def chat(
        self,
        *,
        client: httpx.AsyncClient,
        messages: list[ChatMessage],
        model: str | None,
        temperature: float,
        max_tokens: int,
    ) -> ChatResponse:
        chosen_model = model or self.default_model
        url = f"{self.base_url}/api/chat"
        payload = {
            "model": chosen_model,
            "messages": [m.model_dump() for m in messages],
            "stream": False,
            "options": {
                "temperature": temperature,
                "num_predict": max_tokens,
            },
        }
        try:
            response = await client.post(url, json=payload)
        except httpx.HTTPError as exc:
            raise ProviderError(
                self.name,
                f"could not reach Ollama at {self.base_url}: {exc}",
                status_code=503,
            ) from exc
        _raise_for_http_error(self.name, response)
        body = response.json()
        if not isinstance(body, dict):
            raise ProviderError(self.name, "non-object response body")
        message = body.get("message") or {}
        if not isinstance(message, dict) or not isinstance(message.get("content"), str):
            raise ProviderError(self.name, "missing message content")
        return self._make_response(self.name, chosen_model, message["content"])
