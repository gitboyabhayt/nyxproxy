from __future__ import annotations

import pytest
from fastapi.testclient import TestClient


def test_lists_all_providers(client: TestClient) -> None:
    response = client.get("/v1/providers")
    assert response.status_code == 200
    body = response.json()
    names = {p["name"] for p in body["providers"]}
    assert {
        "groq",
        "openrouter",
        "huggingface",
        "cloudflare",
        "github_models",
        "nvidia",
        "bytez",
        "gemini",
        "ollama",
    } <= names
    # Without env vars, Ollama is always advertised; others should be unavailable.
    by_name = {p["name"]: p for p in body["providers"]}
    assert by_name["groq"]["available"] is False
    assert by_name["ollama"]["available"] is True


@pytest.mark.parametrize(
    "provider",
    [
        "groq",
        "openrouter",
        "huggingface",
        "cloudflare",
        "github_models",
        "nvidia",
        "bytez",
        "gemini",
    ],
)
def test_chat_without_key_returns_503(client: TestClient, provider: str) -> None:
    response = client.post(
        "/v1/chat/completions",
        json={
            "provider": provider,
            "messages": [{"role": "user", "content": "ping"}],
        },
    )
    assert response.status_code == 503
    assert "no API key" in response.json()["detail"] or "needs both" in response.json()["detail"]


def test_unknown_provider_returns_404(client: TestClient) -> None:
    response = client.post(
        "/v1/chat/completions",
        json={
            "provider": "does-not-exist",
            "messages": [{"role": "user", "content": "ping"}],
        },
    )
    assert response.status_code == 404
