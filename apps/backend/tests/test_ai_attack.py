"""Tests for `/v1/ai/*` endpoints (Features C, H, L) and provider failover."""

from __future__ import annotations

import json
from collections.abc import Iterator

import httpx
import pytest
import respx
from fastapi.testclient import TestClient

from nyxproxy_backend.config import get_settings
from nyxproxy_backend.main import create_app


@pytest.fixture
def client(monkeypatch: pytest.MonkeyPatch) -> Iterator[TestClient]:
    monkeypatch.setenv("GROQ_API_KEY", "test-groq")
    monkeypatch.setenv("OPENROUTER_API_KEY", "test-openrouter")
    monkeypatch.setenv("NYXPROXY_DEFAULT_PROVIDER", "groq")
    get_settings.cache_clear()
    app = create_app()
    with TestClient(app) as c:
        yield c


@respx.mock(assert_all_called=True)
def test_auto_attack_returns_ranked_plan(respx_mock, client: TestClient) -> None:
    plan = {
        "summary": "Login endpoint is susceptible to SQLi.",
        "vectors": [
            {
                "vuln": "sqli",
                "parameter": "u",
                "location": "body",
                "severity": "high",
                "payloads": [
                    {"payload": "' OR 1=1--", "rationale": "auth bypass", "exploitability": 90},
                    {"payload": "admin'--", "rationale": "common", "exploitability": 70},
                ],
            }
        ],
    }
    respx_mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
        return_value=httpx.Response(
            200,
            json={
                "model": "llama-3.3-70b-versatile",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": json.dumps(plan)},
                        "finish_reason": "stop",
                    }
                ],
            },
        )
    )
    resp = client.post(
        "/v1/ai/auto-attack",
        json={
            "request": {
                "method": "POST",
                "url": "https://example.com/login",
                "body": "u=admin&p=admin",
            }
        },
    )
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["provider"] == "groq"
    assert len(body["vectors"]) == 1
    payloads = body["vectors"][0]["payloads"]
    # sorted by exploitability desc
    assert payloads[0]["exploitability"] == 90
    assert payloads[1]["exploitability"] == 70


@respx.mock(assert_all_called=True)
def test_auto_attack_strips_markdown_fence(respx_mock, client: TestClient) -> None:
    plan = {"summary": "x", "vectors": []}
    fenced = "```json\n" + json.dumps(plan) + "\n```"
    respx_mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
        return_value=httpx.Response(
            200,
            json={
                "model": "llama-3.3-70b-versatile",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": fenced},
                        "finish_reason": "stop",
                    }
                ],
            },
        )
    )
    resp = client.post(
        "/v1/ai/auto-attack",
        json={"request": {"method": "GET", "url": "https://x/"}},
    )
    assert resp.status_code == 200
    assert resp.json()["summary"] == "x"


@respx.mock(assert_all_called=True)
def test_failover_groq_429_then_openrouter_ok(respx_mock, client: TestClient) -> None:
    """When primary returns 429, failover should attempt the next provider."""
    respx_mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
        return_value=httpx.Response(429, text="rate limited")
    )
    plan = {"summary": "fallback ok", "vectors": []}
    respx_mock.post("https://openrouter.ai/api/v1/chat/completions").mock(
        return_value=httpx.Response(
            200,
            json={
                "model": "openrouter/auto",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": json.dumps(plan)},
                        "finish_reason": "stop",
                    }
                ],
            },
        )
    )
    resp = client.post(
        "/v1/ai/auto-attack",
        json={"request": {"method": "GET", "url": "https://x/"}},
    )
    assert resp.status_code == 200, resp.text
    body = resp.json()
    assert body["provider"] == "openrouter"
    assert "groq" in body["fallbacks_tried"]


@respx.mock(assert_all_called=True)
def test_fuzz_mutate_deduplicates_and_caps(respx_mock, client: TestClient) -> None:
    mutations = {
        "mutations": [
            {"payload": "<svg/onload=1>", "technique": "tag-swap", "bypasses": ["script"]},
            {"payload": "<svg/onload=1>", "technique": "duplicate"},
            {"payload": "<img src=x onerror=1>", "technique": "img"},
            {"payload": "JaVaScRiPt:1", "technique": "case-shift"},
        ]
    }
    respx_mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
        return_value=httpx.Response(
            200,
            json={
                "model": "llama-3.3-70b-versatile",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": json.dumps(mutations)},
                        "finish_reason": "stop",
                    }
                ],
            },
        )
    )
    resp = client.post(
        "/v1/ai/fuzz-mutate",
        json={"seed": "<script>1</script>", "attack_type": "xss", "count": 3},
    )
    assert resp.status_code == 200
    body = resp.json()
    payloads = [m["payload"] for m in body["mutations"]]
    assert len(payloads) == 3  # capped
    assert len(set(payloads)) == len(payloads)  # deduped


@respx.mock(assert_all_called=True)
def test_chain_scan_clamps_risk_score(respx_mock, client: TestClient) -> None:
    body = {
        "summary": "ok",
        "risk_score": 250,  # out of range, should clamp to 100
        "steps": [
            {"kind": "passive", "title": "headers", "issues": [], "payloads_used": [], "notes": ""}
        ],
        "next_actions": ["check CSP"],
    }
    respx_mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
        return_value=httpx.Response(
            200,
            json={
                "model": "llama-3.3-70b-versatile",
                "choices": [
                    {
                        "index": 0,
                        "message": {"role": "assistant", "content": json.dumps(body)},
                        "finish_reason": "stop",
                    }
                ],
            },
        )
    )
    resp = client.post(
        "/v1/ai/chain-scan",
        json={"request": {"method": "GET", "url": "https://x/"}},
    )
    assert resp.status_code == 200
    out = resp.json()
    assert out["risk_score"] == 100
    assert out["next_actions"] == ["check CSP"]


@respx.mock(assert_all_called=False)
def test_failover_all_fail_returns_503(respx_mock, client: TestClient) -> None:
    """When every credentialed provider fails, the API surfaces 503 with attempts."""
    respx_mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
        return_value=httpx.Response(429, text="rate limited")
    )
    respx_mock.post("https://openrouter.ai/api/v1/chat/completions").mock(
        return_value=httpx.Response(500, text="boom")
    )
    # Ollama is always advertised, so explicitly mock it failing.
    respx_mock.post("http://localhost:11434/api/chat").mock(
        side_effect=httpx.ConnectError("connection refused")
    )
    resp = client.post(
        "/v1/ai/auto-attack",
        json={"request": {"method": "GET", "url": "https://x/"}},
    )
    assert resp.status_code == 503


def test_invalid_json_from_ai_returns_502(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("GROQ_API_KEY", "test-groq")
    monkeypatch.setenv("NYXPROXY_DEFAULT_PROVIDER", "groq")
    get_settings.cache_clear()
    app = create_app()
    with TestClient(app) as c, respx.mock(assert_all_called=True) as mock:
        mock.post("https://api.groq.com/openai/v1/chat/completions").mock(
            return_value=httpx.Response(
                200,
                json={
                    "model": "llama-3.3-70b-versatile",
                    "choices": [
                        {
                            "index": 0,
                            "message": {"role": "assistant", "content": "I cannot help."},
                            "finish_reason": "stop",
                        }
                    ],
                },
            )
        )
        resp = c.post(
            "/v1/ai/auto-attack",
            json={"request": {"method": "GET", "url": "https://x/"}},
        )
        assert resp.status_code == 502
