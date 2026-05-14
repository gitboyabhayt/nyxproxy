"""Provider failover chain.

When the desktop app issues an analysis request and the primary provider is
unavailable, rate-limited, or returns a transient upstream error, we walk a
configured fallback chain so the user is not blocked.

The chain is deterministic and resolved at request time from the live
``providers`` dict on ``app.state``: we filter out providers without
credentials and try each remaining one in order.

This is a *legitimate* failover (one of many publicly-documented free model
gateways may be unreachable from a given network), not an evasion of any
service's TOS. We never retry the same provider with mutated credentials and
we surface the original error if every provider fails.
"""

from __future__ import annotations

from collections.abc import Iterable
from dataclasses import dataclass

import httpx

from ..schemas import ChatMessage, ChatResponse
from .base import Provider, ProviderError

# Order is "most-reliable / free / least-rate-limited first". Tweak by env if
# needed (NYXPROXY_FAILOVER_ORDER comma-separated provider names).
DEFAULT_CHAIN: tuple[str, ...] = (
    "groq",
    "openrouter",
    "gemini",
    "github_models",
    "cloudflare",
    "huggingface",
    "nvidia",
    "bytez",
    "ollama",
)


@dataclass
class FailoverAttempt:
    provider: str
    ok: bool
    status_code: int | None
    error: str | None


@dataclass
class FailoverOutcome:
    response: ChatResponse | None
    attempts: list[FailoverAttempt]

    @property
    def succeeded(self) -> bool:
        return self.response is not None


def resolve_chain(
    providers: dict[str, Provider],
    *,
    preferred: str | None,
    override_order: Iterable[str] | None = None,
) -> list[tuple[str, Provider]]:
    """Build the ordered list of (name, provider) we'll try."""
    order = list(override_order) if override_order else list(DEFAULT_CHAIN)
    if preferred and preferred in providers and preferred not in order:
        order.insert(0, preferred)
    elif preferred and preferred in order:
        order.remove(preferred)
        order.insert(0, preferred)

    out: list[tuple[str, Provider]] = []
    seen: set[str] = set()
    for name in order:
        if name in seen:
            continue
        seen.add(name)
        p = providers.get(name)
        if p is None or not p.available:
            continue
        out.append((name, p))
    return out


async def run_with_failover(
    *,
    client: httpx.AsyncClient,
    providers: dict[str, Provider],
    preferred: str | None,
    model: str | None,
    messages: list[ChatMessage],
    temperature: float = 0.2,
    max_tokens: int = 1500,
    override_order: Iterable[str] | None = None,
) -> FailoverOutcome:
    """Try the resolved chain in order, returning the first success.

    Failures are recorded against each attempt so the caller can surface
    a detailed diagnostic when every provider fails.
    """
    chain = resolve_chain(providers, preferred=preferred, override_order=override_order)
    attempts: list[FailoverAttempt] = []
    if not chain:
        return FailoverOutcome(response=None, attempts=attempts)

    for name, provider in chain:
        try:
            result = await provider.chat(
                client=client,
                messages=messages,
                model=model if name == (preferred or "") else None,
                temperature=temperature,
                max_tokens=max_tokens,
            )
            attempts.append(FailoverAttempt(provider=name, ok=True, status_code=None, error=None))
            return FailoverOutcome(response=result, attempts=attempts)
        except ProviderError as exc:
            attempts.append(
                FailoverAttempt(
                    provider=name,
                    ok=False,
                    status_code=exc.status_code,
                    error=str(exc),
                )
            )
        except httpx.HTTPError as exc:
            attempts.append(
                FailoverAttempt(
                    provider=name,
                    ok=False,
                    status_code=None,
                    error=f"http error: {exc}",
                )
            )
    return FailoverOutcome(response=None, attempts=attempts)
