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
