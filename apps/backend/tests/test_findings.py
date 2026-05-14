from __future__ import annotations

from fastapi.testclient import TestClient


def test_categorize_owasp_sqli(client: TestClient) -> None:
    response = client.get(
        "/findings/categorize-owasp",
        params={"description": "Reflected SQL injection in /search?q="},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["code"] == "A03"
    assert body["title"] == "Injection"
    assert "sql injection" in body["matched_keywords"]


def test_categorize_owasp_ssrf(client: TestClient) -> None:
    response = client.get(
        "/findings/categorize-owasp",
        params={"description": "Server-side request forgery via image proxy"},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["code"] == "A10"


def test_categorize_owasp_jwt(client: TestClient) -> None:
    response = client.get(
        "/findings/categorize-owasp",
        params={"description": "JWT alg=none accepted by /api/login"},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["code"] == "A07"


def test_categorize_owasp_unknown_falls_back(client: TestClient) -> None:
    response = client.get(
        "/findings/categorize-owasp",
        params={"description": "totally unrelated description"},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["code"] == "A05"
    assert body["matched_keywords"] == []


def test_categorize_owasp_requires_description(client: TestClient) -> None:
    response = client.get("/findings/categorize-owasp")
    assert response.status_code == 422


def test_map_cve_log4shell(client: TestClient) -> None:
    response = client.get(
        "/findings/map-cve",
        params={"description": "Log4Shell payload triggers JNDI lookup"},
    )
    assert response.status_code == 200
    body = response.json()
    ids = [c["id"] for c in body["cves"]]
    assert "CVE-2021-44228" in ids


def test_map_cve_sqli(client: TestClient) -> None:
    response = client.get(
        "/findings/map-cve",
        params={"description": "Confirmed SQLi via UNION SELECT"},
    )
    assert response.status_code == 200
    body = response.json()
    ids = [c["id"] for c in body["cves"]]
    assert "CWE-89" in ids


def test_map_cve_no_match_returns_empty(client: TestClient) -> None:
    response = client.get(
        "/findings/map-cve",
        params={"description": "nothing matches here"},
    )
    assert response.status_code == 200
    body = response.json()
    assert body["cves"] == []
    assert body["matched_keywords"] == []
