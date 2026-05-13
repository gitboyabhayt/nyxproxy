"""HuggingFace provider — uses the unified router which is OpenAI-compatible.

This is also the path for community fine-tunes (e.g. CyberMind-style security
specialists) — pass the full ``owner/repo`` model id and the HF router will
forward to the right inference endpoint.
"""

from __future__ import annotations

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, call_openai_compatible


class HuggingFaceProvider(Provider):
    name = "huggingface"
    description = "HuggingFace Inference Router — works with most chat-completion models on the Hub."
    default_model = "meta-llama/Llama-3.3-70B-Instruct"
    endpoint = "https://router.huggingface.co/v1/chat/completions"

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
