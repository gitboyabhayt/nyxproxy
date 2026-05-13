from __future__ import annotations

from collections.abc import Iterator

import httpx
import pytest
import respx
from fastapi.testclient import TestClient

from nyxproxy_backend.config import get_settings
from nyxproxy_backend.main import create_app


@pytest.fixture
def authed_client(monkeypatch: pytest.MonkeyPatch) -> Iterator[TestClient]:
    monkeypatch.setenv("GROQ_API_KEY", "test-groq-key")
    monkeypatch.setenv("NYXPROXY_DEFAULT_PROVIDER", "groq")
    get_settings.cache_clear()
    app = create_app()
    with TestClient(app) as c:
        yield c


@respx.mock(assert_all_called=True)
def test_explain_request_calls_groq(respx_mock, authed_client: TestClient) -> None:
    respx_mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
        return_value=httpx.Response(
            200,
            json={
                "id": "x",
                "model": "llama-3.3-70b-versatile",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": "Login endpoint."},
                        "finish_reason": "stop",
                    }
                ],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15},
            },
        )
    )
    response = authed_client.post(
        "/v1/analyze/request",
        json={
            "request": {
                "method": "POST",
                "url": "https://example.com/api/login",
                "headers": {"Content-Type": "application/json"},
                "body": '{"u":"a","p":"b"}',
            }
        },
    )
    assert response.status_code == 200, response.text
    body = response.json()
    assert body["provider"] == "groq"
    assert body["content"] == "Login endpoint."


@respx.mock(assert_all_called=True)
def test_payloads_endpoint(respx_mock, authed_client: TestClient) -> None:
    respx_mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
        return_value=httpx.Response(
            200,
            json={
                "model": "llama-3.3-70b-versatile",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": "' OR 1=1--\n\"><script>1</script>"},
                        "finish_reason": "stop",
                    }
                ],
            },
        )
    )
    response = authed_client.post(
        "/v1/analyze/payloads",
        json={
            "request": {
                "method": "GET",
                "url": "https://example.com/search?q=foo",
            },
            "parameter": "q",
            "attack_type": "xss",
            "count": 5,
        },
    )
    assert response.status_code == 200
    body = response.json()
    assert "OR 1=1" in body["content"]


@respx.mock(assert_all_called=True)
def test_upstream_error_propagates_as_502(respx_mock, authed_client: TestClient) -> None:
    respx_mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
        return_value=httpx.Response(500, text="internal")
    )
    response = authed_client.post(
        "/v1/analyze/request",
        json={"request": {"method": "GET", "url": "https://x/"}},
    )
    assert response.status_code == 502
