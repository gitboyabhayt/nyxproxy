"""Pydantic schemas shared between the API surface and provider adapters."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, Field

Role = Literal["system", "user", "assistant"]


class ChatMessage(BaseModel):
    role: Role
    content: str


class ChatRequest(BaseModel):
    messages: list[ChatMessage]
    provider: str | None = Field(default=None, description="Explicit provider override.")
    model: str | None = Field(default=None, description="Provider-specific model id.")
    temperature: float = Field(default=0.2, ge=0.0, le=2.0)
    max_tokens: int = Field(default=1024, ge=1, le=8192)
    stream: bool = Field(default=False)


class ChatChoice(BaseModel):
    index: int = 0
    message: ChatMessage
    finish_reason: str | None = None


class ChatUsage(BaseModel):
    prompt_tokens: int | None = None
    completion_tokens: int | None = None
    total_tokens: int | None = None


class ChatResponse(BaseModel):
    provider: str
    model: str
    choices: list[ChatChoice]
    usage: ChatUsage | None = None


class ProviderInfo(BaseModel):
    name: str
    available: bool
    default_model: str
    description: str


class ProvidersResponse(BaseModel):
    default: str
    providers: list[ProviderInfo]


class HttpRequestPayload(BaseModel):
    """A captured HTTP request from the proxy core, sent to the AI for analysis."""

    method: str
    url: str
    http_version: str = "HTTP/1.1"
    headers: dict[str, str] = Field(default_factory=dict)
    body: str | None = None


class HttpResponsePayload(BaseModel):
    status: int
    http_version: str = "HTTP/1.1"
    headers: dict[str, str] = Field(default_factory=dict)
    body: str | None = None


class AnalyzeRequestBody(BaseModel):
    request: HttpRequestPayload
    response: HttpResponsePayload | None = None
    provider: str | None = None
    model: str | None = None


class PayloadRequestBody(BaseModel):
    request: HttpRequestPayload
    parameter: str = Field(description="The header/query/body parameter to fuzz.")
    attack_type: Literal[
        "sqli", "xss", "ssrf", "lfi", "rce", "open_redirect", "ssti", "xxe", "auth_bypass", "auto"
    ] = "auto"
    count: int = Field(default=15, ge=1, le=100)
    provider: str | None = None
    model: str | None = None


class AnalyzeResponse(BaseModel):
    provider: str
    model: str
    content: str
