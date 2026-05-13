"""Provider registry."""

from __future__ import annotations

from ..config import Settings
from .base import Provider, ProviderError
from .bytez import BytezProvider
from .cloudflare import CloudflareProvider
from .gemini import GeminiProvider
from .github_models import GithubModelsProvider
from .groq import GroqProvider
from .huggingface import HuggingFaceProvider
from .nvidia import NvidiaProvider
from .ollama import OllamaProvider
from .openrouter import OpenRouterProvider

__all__ = [
    "Provider",
    "ProviderError",
    "build_providers",
]


def build_providers(settings: Settings) -> dict[str, Provider]:
    """Construct one Provider instance per supported backend.

    Providers without credentials are still instantiated so that the desktop
    app can list them and present a helpful "missing key" message.
    """
    return {
        "groq": GroqProvider(api_key=settings.groq_api_key),
        "openrouter": OpenRouterProvider(api_key=settings.openrouter_api_key),
        "huggingface": HuggingFaceProvider(api_key=settings.hf_token),
        "cloudflare": CloudflareProvider(
            api_key=settings.cloudflare_api_token,
            account_id=settings.cloudflare_account_id,
        ),
        "github_models": GithubModelsProvider(api_key=settings.github_models_token),
        "nvidia": NvidiaProvider(api_key=settings.nvidia_api_key),
        "bytez": BytezProvider(api_key=settings.bytez_api_key),
        "gemini": GeminiProvider(api_key=settings.gemini_api_key),
        "ollama": OllamaProvider(base_url=settings.ollama_base_url),
    }
