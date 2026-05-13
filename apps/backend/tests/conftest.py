"""Test fixtures."""

from __future__ import annotations

from collections.abc import Iterator

import pytest
from fastapi.testclient import TestClient

from nyxproxy_backend.config import Settings, get_settings
from nyxproxy_backend.main import create_app

_PROVIDER_KEYS = (
    "GROQ_API_KEY",
    "OPENROUTER_API_KEY",
    "HF_TOKEN",
    "HUGGINGFACE_API_KEY",
    "BYTEZ_API_KEY",
    "CLOUDFLARE_API_TOKEN",
    "CLOUDFLARE_ACCOUNT_ID",
    "GITHUB_MODELS_TOKEN",
    "NVIDIA_API_KEY",
    "GEMINI_API_KEY",
    "OLLAMA_BASE_URL",
)


@pytest.fixture(autouse=True)
def _clear_settings_cache(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("NYXPROXY_ALLOWED_ORIGINS", "*")
    # Always strip provider env so the test suite is deterministic regardless
    # of whatever the developer has in their local .env.
    for key in _PROVIDER_KEYS:
        monkeypatch.delenv(key, raising=False)

    # Pydantic-Settings loads the project .env file on init, which on dev
    # machines may have real keys. Skip the file altogether for tests.
    original_init = Settings.__init__

    def _no_dotenv_init(self: Settings, **kwargs: object) -> None:  # type: ignore[override]
        kwargs.setdefault("_env_file", None)
        original_init(self, **kwargs)

    monkeypatch.setattr(Settings, "__init__", _no_dotenv_init)
    get_settings.cache_clear()


@pytest.fixture
def client() -> Iterator[TestClient]:
    app = create_app()
    with TestClient(app) as c:
        yield c
