#!/usr/bin/env python3
"""Example NyxProxy plugin — flags WordPress fingerprints.

Plugins are stateless. NyxProxy spawns this process, writes a single line of
JSON-RPC 2.0 on stdin, and expects a single JSON line on stdout. The request
looks like:

    {"jsonrpc":"2.0","id":1,"method":"scan_flow","params":{"flow":{...}}}

and the expected response looks like:

    {"jsonrpc":"2.0","id":1,"result":{"issues":[{...}]}}

where each issue follows the same shape as the built-in passive scanner's
`Issue` struct.
"""

from __future__ import annotations

import base64
import json
import sys
import uuid

WORDPRESS_PATH_HINTS = ("/wp-login.php", "/wp-admin", "/wp-content/", "/wp-includes/")
WORDPRESS_GENERATOR_RE = ("wordpress", "wp-")


def _read_request() -> dict:
    line = sys.stdin.readline()
    if not line.strip():
        return {}
    return json.loads(line)


def _b64decode(body_b64: str) -> bytes:
    if not body_b64:
        return b""
    try:
        return base64.b64decode(body_b64, validate=False)
    except Exception:
        return b""


def _make_issue(flow: dict, evidence: str, description: str) -> dict:
    return {
        "id": str(uuid.uuid4()),
        "flow_id": flow.get("id", ""),
        "rule_id": "wordpress-fingerprint",
        "name": "WordPress installation fingerprinted",
        "severity": "low",
        "confidence": "firm",
        "description": description,
        "evidence": evidence,
        "remediation": (
            "If this app is meant to be private, restrict access to /wp-admin "
            "and /wp-login.php, and remove the `Generator` meta tag from "
            "themes."
        ),
        "host": flow.get("request", {}).get("authority", ""),
        "path": flow.get("request", {}).get("path", ""),
    }


def scan_flow(flow: dict) -> list[dict]:
    request = flow.get("request", {}) or {}
    response = flow.get("response") or {}

    issues: list[dict] = []

    path = (request.get("path") or "").lower()
    for hint in WORDPRESS_PATH_HINTS:
        if hint in path:
            issues.append(
                _make_issue(
                    flow,
                    f"request URL contains '{hint}'",
                    "Request path matches a well-known WordPress route.",
                )
            )
            break

    headers = response.get("headers") or []
    for header in headers:
        name = (header.get("name") or "").lower()
        value = (header.get("value") or "").lower()
        if name == "x-pingback":
            issues.append(
                _make_issue(
                    flow,
                    f"x-pingback: {header.get('value')}",
                    "Response advertises the WordPress XML-RPC pingback endpoint.",
                )
            )
        if name == "server" and "apache" in value:
            # Not a WP fingerprint on its own but useful breadcrumb.
            pass

    body = _b64decode(response.get("body_b64") or "")
    if body:
        body_text = body.decode("utf-8", errors="replace").lower()
        if 'name="generator"' in body_text and any(
            tag in body_text for tag in WORDPRESS_GENERATOR_RE
        ):
            issues.append(
                _make_issue(
                    flow,
                    "<meta name=\"generator\" content=\"WordPress …\">",
                    "Response body contains a WordPress generator meta tag.",
                )
            )

    return issues


def main() -> None:
    request = _read_request()
    method = request.get("method")
    request_id = request.get("id", 1)
    if method != "scan_flow":
        response = {
            "jsonrpc": "2.0",
            "id": request_id,
            "error": {"code": -32601, "message": f"unknown method '{method}'"},
        }
    else:
        flow = (request.get("params") or {}).get("flow") or {}
        try:
            issues = scan_flow(flow)
            response = {
                "jsonrpc": "2.0",
                "id": request_id,
                "result": {"issues": issues},
            }
        except Exception as exc:  # noqa: BLE001 — plugin must never raise
            response = {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32000, "message": str(exc)},
            }
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()


if __name__ == "__main__":
    main()
