"""AI-orchestrated attack / scan / fuzz endpoints (Features C, H, L).

Each endpoint accepts a captured request (and optional context) and calls
the configured providers through the failover chain in
``nyxproxy_backend.providers.failover``. Responses are parsed into typed
Pydantic models so the desktop client never has to handle raw LLM text.
"""

from __future__ import annotations

import json
import re

from fastapi import APIRouter, HTTPException, Request

from ..prompts import (
    build_auto_attack_prompt,
    build_chain_scan_prompt,
    build_fuzz_mutate_prompt,
)
from ..providers.failover import run_with_failover
from ..schemas import (
    AttackPayload,
    AttackVector,
    AutoAttackPlan,
    AutoAttackRequestBody,
    ChainScanRequestBody,
    ChainScanResponse,
    ChainScanStep,
    ChatMessage,
    FuzzMutateRequestBody,
    FuzzMutateResponse,
    FuzzMutation,
)

router = APIRouter(prefix="/v1/ai", tags=["ai-attack"])


_JSON_FENCE = re.compile(r"```(?:json)?\s*(.*?)```", re.DOTALL)


def _extract_json(content: str) -> dict[str, object]:
    """Some providers wrap JSON in markdown fences despite our system prompt.
    Strip them and return the parsed object."""
    body = content.strip()
    match = _JSON_FENCE.search(body)
    if match:
        body = match.group(1).strip()
    # Some providers prepend an explanation; try to locate the first {.
    if not body.startswith("{"):
        first = body.find("{")
        last = body.rfind("}")
        if first >= 0 and last > first:
            body = body[first : last + 1]
    try:
        parsed = json.loads(body)
    except json.JSONDecodeError as exc:
        raise HTTPException(
            status_code=502,
            detail=f"AI provider returned non-JSON output: {exc}; raw={content[:300]!r}",
        ) from exc
    if not isinstance(parsed, dict):
        raise HTTPException(status_code=502, detail="AI provider returned non-object JSON")
    return parsed


def _as_str_list(value: object) -> list[str]:
    if isinstance(value, list):
        return [str(v) for v in value if isinstance(v, (str, int, float))]
    return []


@router.post("/auto-attack", response_model=AutoAttackPlan)
async def auto_attack(body: AutoAttackRequestBody, request: Request) -> AutoAttackPlan:
    """Generate a ranked attack plan against a captured request (Feature C)."""
    prompt = build_auto_attack_prompt(
        body.request, body.response, body.suspected, body.payloads_per_class
    )
    messages = [ChatMessage(role=m["role"], content=m["content"]) for m in prompt]  # type: ignore[arg-type]
    outcome = await run_with_failover(
        client=request.app.state.http_client,
        providers=request.app.state.providers,
        preferred=body.provider,
        model=body.model,
        messages=messages,
        temperature=0.2,
        max_tokens=2000,
    )
    if not outcome.succeeded or outcome.response is None:
        raise HTTPException(
            status_code=503,
            detail={
                "error": "no provider succeeded",
                "attempts": [a.__dict__ for a in outcome.attempts],
            },
        )
    raw = outcome.response.choices[0].message.content
    data = _extract_json(raw)
    vectors_raw = data.get("vectors", [])
    if not isinstance(vectors_raw, list):
        vectors_raw = []
    vectors: list[AttackVector] = []
    for v in vectors_raw:
        if not isinstance(v, dict):
            continue
        payloads_raw = v.get("payloads", [])
        payloads: list[AttackPayload] = []
        if isinstance(payloads_raw, list):
            for p in payloads_raw:
                if not isinstance(p, dict):
                    continue
                try:
                    payloads.append(
                        AttackPayload(
                            payload=str(p.get("payload", "")),
                            rationale=str(p.get("rationale", "")),
                            exploitability=max(0, min(100, int(p.get("exploitability", 50)))),
                        )
                    )
                except (TypeError, ValueError):
                    continue
        # Sort payloads by exploitability desc.
        payloads.sort(key=lambda p: p.exploitability, reverse=True)
        try:
            vectors.append(
                AttackVector(
                    vuln=v.get("vuln", "sqli"),  # type: ignore[arg-type]
                    parameter=str(v.get("parameter", "")),
                    location=v.get("location", "query"),  # type: ignore[arg-type]
                    severity=v.get("severity", "medium"),  # type: ignore[arg-type]
                    payloads=payloads,
                )
            )
        except Exception:  # pragma: no cover  - validation guard
            continue
    return AutoAttackPlan(
        summary=str(data.get("summary", "")),
        vectors=vectors,
        provider=outcome.response.provider,
        model=outcome.response.model,
        fallbacks_tried=[a.provider for a in outcome.attempts if not a.ok],
    )


@router.post("/fuzz-mutate", response_model=FuzzMutateResponse)
async def fuzz_mutate(body: FuzzMutateRequestBody, request: Request) -> FuzzMutateResponse:
    """Generate filter-bypass mutations for a single seed (Feature L)."""
    prompt = build_fuzz_mutate_prompt(body.seed, body.parameter, body.attack_type, body.count)
    messages = [ChatMessage(role=m["role"], content=m["content"]) for m in prompt]  # type: ignore[arg-type]
    outcome = await run_with_failover(
        client=request.app.state.http_client,
        providers=request.app.state.providers,
        preferred=body.provider,
        model=body.model,
        messages=messages,
        temperature=0.4,
        max_tokens=2500,
    )
    if not outcome.succeeded or outcome.response is None:
        raise HTTPException(
            status_code=503,
            detail={
                "error": "no provider succeeded",
                "attempts": [a.__dict__ for a in outcome.attempts],
            },
        )
    raw = outcome.response.choices[0].message.content
    data = _extract_json(raw)
    mutations_raw = data.get("mutations", [])
    mutations: list[FuzzMutation] = []
    if isinstance(mutations_raw, list):
        seen_payloads: set[str] = set()
        for m in mutations_raw:
            if not isinstance(m, dict):
                continue
            payload = str(m.get("payload", "")).strip()
            if not payload or payload in seen_payloads:
                continue
            seen_payloads.add(payload)
            mutations.append(
                FuzzMutation(
                    payload=payload,
                    technique=str(m.get("technique", "")),
                    bypasses=_as_str_list(m.get("bypasses")),
                )
            )
    return FuzzMutateResponse(
        mutations=mutations[: body.count],
        provider=outcome.response.provider,
        model=outcome.response.model,
        fallbacks_tried=[a.provider for a in outcome.attempts if not a.ok],
    )


@router.post("/chain-scan", response_model=ChainScanResponse)
async def chain_scan(body: ChainScanRequestBody, request: Request) -> ChainScanResponse:
    """AI-driven chained scan: passive → active → reporter (Feature H)."""
    prompt = build_chain_scan_prompt(body.request, body.response, body.issues_seen)
    messages = [ChatMessage(role=m["role"], content=m["content"]) for m in prompt]  # type: ignore[arg-type]
    outcome = await run_with_failover(
        client=request.app.state.http_client,
        providers=request.app.state.providers,
        preferred=body.provider,
        model=body.model,
        messages=messages,
        temperature=0.2,
        max_tokens=2200,
    )
    if not outcome.succeeded or outcome.response is None:
        raise HTTPException(
            status_code=503,
            detail={
                "error": "no provider succeeded",
                "attempts": [a.__dict__ for a in outcome.attempts],
            },
        )
    raw = outcome.response.choices[0].message.content
    data = _extract_json(raw)
    steps_raw = data.get("steps", [])
    steps: list[ChainScanStep] = []
    if isinstance(steps_raw, list):
        for s in steps_raw:
            if not isinstance(s, dict):
                continue
            try:
                steps.append(
                    ChainScanStep(
                        kind=s.get("kind", "passive"),  # type: ignore[arg-type]
                        title=str(s.get("title", "")),
                        issues=_as_str_list(s.get("issues")),
                        payloads_used=_as_str_list(s.get("payloads_used")),
                        notes=str(s.get("notes", "")),
                    )
                )
            except Exception:  # pragma: no cover
                continue
    next_actions = _as_str_list(data.get("next_actions"))
    risk = data.get("risk_score", 0)
    try:
        risk = max(0, min(100, int(risk)))
    except (TypeError, ValueError):
        risk = 0
    return ChainScanResponse(
        summary=str(data.get("summary", "")),
        risk_score=risk,
        steps=steps,
        next_actions=next_actions,
        provider=outcome.response.provider,
        model=outcome.response.model,
        fallbacks_tried=[a.provider for a in outcome.attempts if not a.ok],
    )
