"""Cloudflare Workers AI — OpenAI-compatible endpoint scoped to an account."""

from __future__ import annotations

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, ProviderError, call_openai_compatible


class CloudflareProvider(Provider):
    name = "cloudflare"
    description = "Cloudflare Workers AI — free tier across many open models."
    default_model = "@cf/meta/llama-3.3-70b-instruct-fp8-fast"

    def __init__(self, *, api_key: str | None = None, account_id: str | None = None) -> None:
        super().__init__(api_key=api_key)
        self.account_id = account_id

    @property
    def available(self) -> bool:
        return bool(self.api_key) and bool(self.account_id)

    async def chat(
        self,
        *,
        client: httpx.AsyncClient,
        messages: list[ChatMessage],
        model: str | None,
        temperature: float,
        max_tokens: int,
    ) -> ChatResponse:
        if not self.api_key or not self.account_id:
            raise ProviderError(
                self.name,
                "needs both CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID",
                status_code=503,
            )
        url = (
            f"https://api.cloudflare.com/client/v4/accounts/{self.account_id}"
            "/ai/v1/chat/completions"
        )
        return await call_openai_compatible(
            client=client,
            provider_name=self.name,
            url=url,
            headers={
                "Authorization": f"Bearer {self.api_key}",
                "Content-Type": "application/json",
            },
            messages=messages,
            model=model or self.default_model,
            temperature=temperature,
            max_tokens=max_tokens,
        )
