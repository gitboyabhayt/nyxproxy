"""Reusable system prompts for the AI assistant features."""

from __future__ import annotations

from .schemas import HttpRequestPayload, HttpResponsePayload


def _format_headers(headers: dict[str, str]) -> str:
    if not headers:
        return "(none)"
    return "\n".join(f"{k}: {v}" for k, v in headers.items())


def render_request(req: HttpRequestPayload) -> str:
    body = req.body if req.body is not None else ""
    return (
        f"{req.method} {req.url} {req.http_version}\n{_format_headers(req.headers)}\n\n{body}"
    ).strip()


def render_response(resp: HttpResponsePayload | None) -> str:
    if resp is None:
        return "(no response captured)"
    body = resp.body if resp.body is not None else ""
    return (f"{resp.http_version} {resp.status}\n{_format_headers(resp.headers)}\n\n{body}").strip()


EXPLAIN_SYSTEM = (
    "You are NyxProxy's senior application-security analyst. Given a single HTTP "
    "request (and optionally its response), write a concise plain-English "
    "explanation suitable for a security tester. Cover: the endpoint's purpose, "
    "noteworthy parameters and how the server appears to use them, observed "
    "authentication/session handling, content types, and anything unusual. "
    "Be specific. No filler. Use compact markdown."
)


VULNS_SYSTEM = (
    "You are NyxProxy's offensive security advisor. Given an HTTP request "
    "(and response if available), enumerate the most plausible vulnerability "
    "classes a tester should probe next. For each, give: (1) a short name, "
    "(2) why this request hints at it, (3) a concrete next test (curl-style "
    "snippet or parameter to manipulate). Sort by likelihood, top 6 only. "
    "Markdown table or numbered list."
)


PAYLOADS_SYSTEM = (
    "You are NyxProxy's payload generator. Produce a clean newline-separated "
    "list of fuzzing payloads tailored to the request, parameter, and attack "
    "type provided. NEVER include numbering, commentary, or markdown — just "
    "raw payload strings, one per line. Avoid duplicates. Bias toward "
    "payloads that bypass simple WAFs."
)


def build_explain_prompt(
    req: HttpRequestPayload, resp: HttpResponsePayload | None
) -> list[dict[str, str]]:
    return [
        {"role": "system", "content": EXPLAIN_SYSTEM},
        {
            "role": "user",
            "content": (
                f"--- REQUEST ---\n{render_request(req)}\n\n"
                f"--- RESPONSE ---\n{render_response(resp)}"
            ),
        },
    ]


def build_vulns_prompt(
    req: HttpRequestPayload, resp: HttpResponsePayload | None
) -> list[dict[str, str]]:
    return [
        {"role": "system", "content": VULNS_SYSTEM},
        {
            "role": "user",
            "content": (
                f"--- REQUEST ---\n{render_request(req)}\n\n"
                f"--- RESPONSE ---\n{render_response(resp)}"
            ),
        },
    ]


def build_payloads_prompt(
    req: HttpRequestPayload, parameter: str, attack_type: str, count: int
) -> list[dict[str, str]]:
    return [
        {"role": "system", "content": PAYLOADS_SYSTEM},
        {
            "role": "user",
            "content": (
                f"Attack type: {attack_type}\n"
                f"Target parameter: {parameter}\n"
                f"Number of payloads: {count}\n\n"
                f"--- REQUEST ---\n{render_request(req)}"
            ),
        },
    ]


# ---------------------------------------------------------------------------
# AI auto-attack / chained scan / fuzz mutator (PR #6)
# ---------------------------------------------------------------------------


AUTO_ATTACK_SYSTEM = (
    "You are NyxProxy's automated attack planner. Given a single HTTP request "
    "(and optionally its response), output a JSON object describing an "
    "ordered, prioritised attack plan a tester can execute next. "
    "Schema:\n"
    '{"summary": str, "vectors": [\n'
    '  {"vuln": one of [sqli|xss|ssrf|lfi|rce|open_redirect|ssti|xxe|auth_bypass|idor|csrf|jwt|deserialization|graphql_injection|nosql|log4shell|prototype_pollution|race_condition],\n'
    '   "parameter": str, "location": one of [query|body|header|cookie|path],\n'
    '   "severity": one of [info|low|medium|high|critical],\n'
    '   "payloads": [ {"payload": str, "rationale": str, "exploitability": 0-100 } ]\n'
    "  }\n"
    "]}\n"
    "Output STRICT, parseable JSON. No markdown fence, no commentary."
)


FUZZ_MUTATE_SYSTEM = (
    "You are NyxProxy's AI fuzz mutator. Given a single seed payload, "
    "generate variations that explore filter-bypass techniques (encoding, "
    "case shifting, comment insertion, alternative keywords, parameter "
    "pollution, unicode normalisation, etc.). Output STRICT JSON of shape "
    '{"mutations": [{"payload": str, "technique": str, "bypasses": [str]}]}.\n'
    "No prose, no markdown."
)


CHAIN_SCAN_SYSTEM = (
    "You are NyxProxy's chained scan coordinator. You receive a captured "
    "request/response and a list of passive issues already observed. Plan "
    "the next steps as: 1) any further passive checks, 2) active probes to "
    "run (with concrete payloads), 3) what should land in the final report. "
    "Output STRICT JSON of shape "
    '{"summary": str, "risk_score": 0-100, '
    '"steps": [{"kind": one of [passive|active|report], "title": str, '
    '"issues": [str], "payloads_used": [str], "notes": str}], '
    '"next_actions": [str]}.\n'
    "No prose outside JSON."
)


def build_auto_attack_prompt(
    req: HttpRequestPayload,
    resp: HttpResponsePayload | None,
    suspected: list[str] | None,
    payloads_per_class: int,
) -> list[dict[str, str]]:
    constraint = f"Restrict the plan to vuln classes: {', '.join(suspected)}\n" if suspected else ""
    return [
        {"role": "system", "content": AUTO_ATTACK_SYSTEM},
        {
            "role": "user",
            "content": (
                f"{constraint}"
                f"Target payloads per vuln class: {payloads_per_class}\n\n"
                f"--- REQUEST ---\n{render_request(req)}\n\n"
                f"--- RESPONSE ---\n{render_response(resp)}"
            ),
        },
    ]


def build_fuzz_mutate_prompt(
    seed: str, parameter: str | None, attack_type: str, count: int
) -> list[dict[str, str]]:
    return [
        {"role": "system", "content": FUZZ_MUTATE_SYSTEM},
        {
            "role": "user",
            "content": (
                f"Seed payload: {seed!r}\n"
                f"Target parameter: {parameter or '(any)'}\n"
                f"Attack class: {attack_type}\n"
                f"Generate exactly {count} unique mutations."
            ),
        },
    ]


def build_chain_scan_prompt(
    req: HttpRequestPayload,
    resp: HttpResponsePayload | None,
    issues_seen: list[str],
) -> list[dict[str, str]]:
    seen = "\n".join(f"- {i}" for i in issues_seen) if issues_seen else "(none)"
    return [
        {"role": "system", "content": CHAIN_SCAN_SYSTEM},
        {
            "role": "user",
            "content": (
                f"--- PASSIVE ISSUES ALREADY SEEN ---\n{seen}\n\n"
                f"--- REQUEST ---\n{render_request(req)}\n\n"
                f"--- RESPONSE ---\n{render_response(resp)}"
            ),
        },
    ]
