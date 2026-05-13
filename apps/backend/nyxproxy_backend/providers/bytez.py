"""Bytez provider — has its own JSON schema, not OpenAI-compatible."""

from __future__ import annotations

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, ProviderError, _raise_for_http_error


class BytezProvider(Provider):
    name = "bytez"
    description = "Bytez — model marketplace, exposes a unified inference API."
    default_model = "meta-llama/Llama-3.3-70B-Instruct"

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
        chosen_model = model or self.default_model
        url = f"https://api.bytez.com/models/v2/{chosen_model}"
        payload = {
            "messages": [m.model_dump() for m in messages],
            "params": {
                "temperature": temperature,
                "max_new_tokens": max_tokens,
            },
        }
        response = await client.post(
            url,
            headers={
                "Authorization": f"Key {self.api_key}",
                "Content-Type": "application/json",
            },
            json=payload,
        )
        _raise_for_http_error(self.name, response)
        body = response.json()
        if not isinstance(body, dict):
            raise ProviderError(self.name, "non-object response body")

        error = body.get("error")
        if error:
            raise ProviderError(self.name, str(error))

        output = body.get("output")
        content: str | None = None
        if isinstance(output, str):
            content = output
        elif isinstance(output, dict):
            inner = output.get("content")
            if isinstance(inner, str):
                content = inner
        elif isinstance(output, list) and output:
            first = output[0]
            if isinstance(first, dict) and isinstance(first.get("content"), str):
                content = first["content"]
            elif isinstance(first, str):
                content = first
        if content is None:
            raise ProviderError(self.name, "could not parse Bytez response")
        return self._make_response(self.name, chosen_model, content)
