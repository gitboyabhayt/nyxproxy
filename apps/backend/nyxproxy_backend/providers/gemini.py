"""Google Gemini provider — converts OpenAI-style messages to Gemini ``contents``."""

from __future__ import annotations

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, ProviderError, _raise_for_http_error


class GeminiProvider(Provider):
    name = "gemini"
    description = "Google Gemini API — Flash and Pro models, free tier available."
    default_model = "gemini-2.0-flash"

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

        system_chunks: list[str] = []
        contents: list[dict[str, object]] = []
        for msg in messages:
            if msg.role == "system":
                system_chunks.append(msg.content)
                continue
            role = "user" if msg.role == "user" else "model"
            contents.append(
                {"role": role, "parts": [{"text": msg.content}]},
            )

        payload: dict[str, object] = {
            "contents": contents,
            "generationConfig": {
                "temperature": temperature,
                "maxOutputTokens": max_tokens,
            },
        }
        if system_chunks:
            payload["systemInstruction"] = {
                "role": "system",
                "parts": [{"text": "\n\n".join(system_chunks)}],
            }

        url = (
            f"https://generativelanguage.googleapis.com/v1beta/models/"
            f"{chosen_model}:generateContent"
        )
        response = await client.post(
            url,
            headers={"Content-Type": "application/json"},
            params={"key": self.api_key},
            json=payload,
        )
        _raise_for_http_error(self.name, response)
        body = response.json()
        if not isinstance(body, dict):
            raise ProviderError(self.name, "non-object response body")

        candidates = body.get("candidates") or []
        if not isinstance(candidates, list) or not candidates:
            raise ProviderError(self.name, "no candidates returned")
        first = candidates[0]
        if not isinstance(first, dict):
            raise ProviderError(self.name, "malformed candidate")
        content_block = first.get("content")
        if not isinstance(content_block, dict):
            raise ProviderError(self.name, "no content in candidate")
        parts = content_block.get("parts") or []
        if not isinstance(parts, list):
            raise ProviderError(self.name, "malformed parts list")
        text = "".join(part.get("text", "") for part in parts if isinstance(part, dict))
        return self._make_response(
            self.name,
            chosen_model,
            text,
            finish_reason=first.get("finishReason")
            if isinstance(first.get("finishReason"), str)
            else None,
        )
