"""Findings enrichment endpoints.

Two pure-Python routes that take a short description of a security finding
(or its rule identifier) and return:

* `/findings/categorize-owasp` — the matching OWASP Top 10 (2021) category.
* `/findings/map-cve` — a list of well-known CVE / CWE identifiers most
  closely associated with the same vuln class.

Both endpoints are deterministic and offline — no external API calls.  They
exist to keep the rule -> framework mapping out of every individual rule
implementation and to give the Tauri client a consistent way to enrich
imported third-party findings.
"""

from __future__ import annotations

import re
from typing import Final

from fastapi import APIRouter, Query
from pydantic import BaseModel

router = APIRouter(prefix="/findings", tags=["findings"])


class OwaspCategory(BaseModel):
    code: str
    title: str
    description: str
    matched_keywords: list[str]


class CveHint(BaseModel):
    id: str
    title: str
    cwe: str | None = None
    severity: str | None = None
    url: str


class CveMapping(BaseModel):
    matched_keywords: list[str]
    cves: list[CveHint]


# OWASP Top 10 (2021) — keyed by canonical 3-char code.
_OWASP: Final[dict[str, tuple[str, str, list[str]]]] = {
    "A01": (
        "Broken Access Control",
        "Failures that allow attackers to act outside their intended permissions.",
        [
            "idor",
            "broken access",
            "missing authz",
            "missing authorization",
            "privilege escalation",
            "directory traversal",
            "path traversal",
            "force browsing",
        ],
    ),
    "A02": (
        "Cryptographic Failures",
        "Cryptography missing, weak, or misused — including sensitive data exposure.",
        [
            "plaintext password",
            "weak crypto",
            "tls",
            "ssl",
            "rc4",
            "md5",
            "sha1",
            "missing hsts",
            "missing https",
            "sensitive data exposure",
        ],
    ),
    "A03": (
        "Injection",
        "Untrusted data interpreted as code or command (SQLi, XSS, command, LDAP).",
        [
            "sql injection",
            "sqli",
            "xss",
            "cross-site scripting",
            "cross site scripting",
            "command injection",
            "rce",
            "ldap injection",
            "nosql injection",
            "xpath injection",
            "template injection",
            "ssti",
        ],
    ),
    "A04": (
        "Insecure Design",
        "Design-level flaws — missing threat modelling or business logic errors.",
        ["insecure design", "business logic", "rate limit", "race condition"],
    ),
    "A05": (
        "Security Misconfiguration",
        "Default credentials, verbose errors, missing headers, exposed admin panels.",
        [
            "default credentials",
            "directory listing",
            "verbose error",
            "missing header",
            "x-frame-options",
            "x-content-type-options",
            "csp",
            "content security policy",
            "cors",
            "debug enabled",
        ],
    ),
    "A06": (
        "Vulnerable and Outdated Components",
        "Use of a library or framework with known vulnerabilities.",
        [
            "outdated",
            "vulnerable component",
            "known cve",
            "deprecated",
            "end of life",
            "eol",
        ],
    ),
    "A07": (
        "Identification and Authentication Failures",
        "Broken authentication, weak passwords, missing MFA, JWT mis-use.",
        [
            "jwt",
            "jwks",
            "alg none",
            "weak password",
            "credential stuffing",
            "missing mfa",
            "session fixation",
            "auth bypass",
        ],
    ),
    "A08": (
        "Software and Data Integrity Failures",
        "Unsigned updates, insecure deserialisation, untrusted CI plugins.",
        [
            "deserialization",
            "deserialisation",
            "unsigned update",
            "supply chain",
            "untrusted",
        ],
    ),
    "A09": (
        "Security Logging and Monitoring Failures",
        "Missing or insufficient logging / alerting on security events.",
        ["missing log", "no logging", "missing audit"],
    ),
    "A10": (
        "Server-Side Request Forgery",
        "Server-side fetch of URLs supplied by the user without validation.",
        ["ssrf", "server-side request forgery", "url fetch"],
    ),
}


# Compact CVE hint catalogue — high-confidence CVE/CWE associations only.
_CVE_HINTS: Final[dict[str, list[CveHint]]] = {
    "sqli": [
        CveHint(
            id="CWE-89",
            title="Improper Neutralisation of Special Elements in an SQL Command",
            cwe="CWE-89",
            severity="critical",
            url="https://cwe.mitre.org/data/definitions/89.html",
        ),
    ],
    "xss": [
        CveHint(
            id="CWE-79",
            title="Improper Neutralisation of Input During Web Page Generation",
            cwe="CWE-79",
            severity="high",
            url="https://cwe.mitre.org/data/definitions/79.html",
        ),
    ],
    "ssrf": [
        CveHint(
            id="CWE-918",
            title="Server-Side Request Forgery (SSRF)",
            cwe="CWE-918",
            severity="high",
            url="https://cwe.mitre.org/data/definitions/918.html",
        ),
    ],
    "rce": [
        CveHint(
            id="CWE-78",
            title="Improper Neutralisation of Special Elements used in an OS Command",
            cwe="CWE-78",
            severity="critical",
            url="https://cwe.mitre.org/data/definitions/78.html",
        ),
    ],
    "command injection": [
        CveHint(
            id="CWE-77",
            title="Improper Neutralisation of Special Elements used in a Command",
            cwe="CWE-77",
            severity="critical",
            url="https://cwe.mitre.org/data/definitions/77.html",
        ),
    ],
    "jwt alg none": [
        CveHint(
            id="CVE-2015-9235",
            title="JSON Web Token (JWT) libraries vulnerable to alg=none confusion",
            cwe="CWE-347",
            severity="high",
            url="https://nvd.nist.gov/vuln/detail/CVE-2015-9235",
        ),
    ],
    "deserialization": [
        CveHint(
            id="CWE-502",
            title="Deserialisation of Untrusted Data",
            cwe="CWE-502",
            severity="critical",
            url="https://cwe.mitre.org/data/definitions/502.html",
        ),
    ],
    "path traversal": [
        CveHint(
            id="CWE-22",
            title="Improper Limitation of a Pathname to a Restricted Directory",
            cwe="CWE-22",
            severity="high",
            url="https://cwe.mitre.org/data/definitions/22.html",
        ),
    ],
    "open redirect": [
        CveHint(
            id="CWE-601",
            title="URL Redirection to Untrusted Site",
            cwe="CWE-601",
            severity="medium",
            url="https://cwe.mitre.org/data/definitions/601.html",
        ),
    ],
    "csrf": [
        CveHint(
            id="CWE-352",
            title="Cross-Site Request Forgery",
            cwe="CWE-352",
            severity="medium",
            url="https://cwe.mitre.org/data/definitions/352.html",
        ),
    ],
    "xxe": [
        CveHint(
            id="CWE-611",
            title="Improper Restriction of XML External Entity Reference",
            cwe="CWE-611",
            severity="high",
            url="https://cwe.mitre.org/data/definitions/611.html",
        ),
    ],
    "log4shell": [
        CveHint(
            id="CVE-2021-44228",
            title="Apache Log4j2 JNDI lookup remote code execution (Log4Shell)",
            cwe="CWE-917",
            severity="critical",
            url="https://nvd.nist.gov/vuln/detail/CVE-2021-44228",
        ),
    ],
    "spring4shell": [
        CveHint(
            id="CVE-2022-22965",
            title="Spring Framework RCE via data binding (Spring4Shell)",
            cwe="CWE-915",
            severity="critical",
            url="https://nvd.nist.gov/vuln/detail/CVE-2022-22965",
        ),
    ],
    "shellshock": [
        CveHint(
            id="CVE-2014-6271",
            title="Bash environment variable command injection (Shellshock)",
            cwe="CWE-78",
            severity="critical",
            url="https://nvd.nist.gov/vuln/detail/CVE-2014-6271",
        ),
    ],
}


_KEYWORD_PATTERN = re.compile(r"[a-z0-9]+")


def _normalise(text: str) -> str:
    return text.lower().strip()


def categorize(text: str) -> OwaspCategory:
    """Return the OWASP Top 10 category that best matches ``text``.

    Always returns a non-null result — falls back to A05 (Security Misconfig)
    if no keyword matches, matching the conservative default used by the
    Rust ``owasp::category_for_rule`` implementation for unknown rule_ids.
    """
    needle = _normalise(text)
    best_code = "A05"
    best_score = 0
    best_kw: list[str] = []
    for code, (_title, _desc, kws) in _OWASP.items():
        matched = [kw for kw in kws if kw in needle]
        if len(matched) > best_score:
            best_score = len(matched)
            best_code = code
            best_kw = matched
    title, desc, _ = _OWASP[best_code]
    return OwaspCategory(
        code=best_code,
        title=title,
        description=desc,
        matched_keywords=best_kw,
    )


def map_cves(text: str) -> CveMapping:
    """Return a list of CVE / CWE hints whose keyword set matches ``text``."""
    needle = _normalise(text)
    seen: dict[str, CveHint] = {}
    matched_kw: list[str] = []
    for kw, hints in _CVE_HINTS.items():
        if kw in needle:
            matched_kw.append(kw)
            for h in hints:
                seen.setdefault(h.id, h)
    return CveMapping(matched_keywords=matched_kw, cves=list(seen.values()))


@router.get(
    "/categorize-owasp",
    response_model=OwaspCategory,
    summary="Map a finding description to OWASP Top 10 (2021)",
)
def categorize_owasp(
    description: str = Query(..., min_length=1, max_length=2048),
) -> OwaspCategory:
    return categorize(description)


@router.get(
    "/map-cve",
    response_model=CveMapping,
    summary="Map a finding description to well-known CVE / CWE identifiers",
)
def map_cve(
    description: str = Query(..., min_length=1, max_length=2048),
) -> CveMapping:
    return map_cves(description)
