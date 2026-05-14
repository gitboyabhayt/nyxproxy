# Security Policy

## Reporting a Vulnerability

NyxProxy is a security testing tool. Treat it like one: do not run untrusted plugins, do not paste secrets into the chat panel, and only intercept traffic on systems you are authorised to test.

If you discover a security vulnerability **in NyxProxy itself** (the desktop app, the Rust core, the backend, or the build pipeline), please report it privately so users can be protected before details become public.

**Preferred channel:** open a [GitHub Security Advisory](https://github.com/gitboyabhayt/nyxproxy/security/advisories/new) on this repository.

**Fallback:** email the maintainer (see GitHub profile) with the subject `[security] NyxProxy: <one-line summary>`.

Please include:

1. NyxProxy version (see Help → About or `~/.nyxproxy/version.txt`)
2. Operating system + architecture
3. Reproduction steps (a minimal proof-of-concept request, crash log, or core dump is ideal)
4. Impact assessment as you see it (RCE, info-disclosure, DoS, etc.)
5. Whether the issue has been disclosed elsewhere

We aim to acknowledge new reports within **72 hours** and to publish a fix or mitigation advisory within **14 days** for high/critical issues. Lower-severity issues are usually resolved in the next minor release.

## Scope

In scope:

* Remote code execution in the desktop app, the backend (`nyxproxy_backend`), or any bundled plugin
* TLS bypass / certificate validation flaws in the proxy core
* CA-key disclosure (the on-disk private key in `~/.nyxproxy/ca/`)
* Privilege escalation through the installer (NSIS, MSI, deb, AppImage)
* SSRF / SSTI / authentication bypass in the FastAPI backend
* Vulnerabilities in dependencies that NyxProxy ships with default configuration that is exploitable

Out of scope (please do **not** report these as security issues):

* Findings reported by NyxProxy *about* a third-party site you tested
* Self-XSS in the request body editor (this is by design — the editor is meant to render attacker-controlled content visually)
* Missing security headers on the hosted backend's `/healthz` endpoint
* Reports without reproduction steps

## Safe Harbor

We will not pursue legal action against researchers who:

* Operate in good faith
* Avoid privacy violations, destruction of data, and degradation of the hosted backend service
* Report findings responsibly via the channels above
* Stop testing as soon as they confirm the issue exists

Thank you for helping keep NyxProxy and its users safe.
