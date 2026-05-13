"""Runtime configuration for the NyxProxy backend.

All settings are loaded from environment variables (or a ``.env`` file at the
project root). Each provider's API key is optional; the backend exposes
whichever providers it has credentials for.
"""

from __future__ import annotations

from functools import lru_cache

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )

    default_provider: str = Field(default="groq", alias="NYXPROXY_DEFAULT_PROVIDER")
    allowed_origins: str = Field(default="*", alias="NYXPROXY_ALLOWED_ORIGINS")
    api_token: str | None = Field(default=None, alias="NYXPROXY_API_TOKEN")
    request_timeout_seconds: float = Field(default=60.0, alias="NYXPROXY_REQUEST_TIMEOUT")

    groq_api_key: str | None = Field(default=None, alias="GROQ_API_KEY")
    openrouter_api_key: str | None = Field(default=None, alias="OPENROUTER_API_KEY")
    hf_token: str | None = Field(default=None, alias="HF_TOKEN")
    cloudflare_api_token: str | None = Field(default=None, alias="CLOUDFLARE_API_TOKEN")
    cloudflare_account_id: str | None = Field(default=None, alias="CLOUDFLARE_ACCOUNT_ID")
    github_models_token: str | None = Field(default=None, alias="GITHUB_MODELS_TOKEN")
    nvidia_api_key: str | None = Field(default=None, alias="NVIDIA_API_KEY")
    bytez_api_key: str | None = Field(default=None, alias="BYTEZ_API_KEY")
    gemini_api_key: str | None = Field(default=None, alias="GEMINI_API_KEY")
    ollama_base_url: str = Field(default="http://localhost:11434", alias="OLLAMA_BASE_URL")

    def origins_list(self) -> list[str]:
        raw = self.allowed_origins.strip()
        if not raw or raw == "*":
            return ["*"]
        return [o.strip() for o in raw.split(",") if o.strip()]


@lru_cache(maxsize=1)
def get_settings() -> Settings:
    return Settings()
