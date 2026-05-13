"""Test fixtures."""

from __future__ import annotations

from collections.abc import Iterator

import pytest
from fastapi.testclient import TestClient

from nyxproxy_backend.config import get_settings
from nyxproxy_backend.main import create_app


@pytest.fixture(autouse=True)
def _clear_settings_cache(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("NYXPROXY_ALLOWED_ORIGINS", "*")
    get_settings.cache_clear()


@pytest.fixture
def client() -> Iterator[TestClient]:
    app = create_app()
    with TestClient(app) as c:
        yield c
