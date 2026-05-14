# NyxProxy GitHub Action (Feature T)

Status: **Shipped** — `gitboyabhayt/nyxproxy/action@main`

## What it does

Runs a headless NyxProxy security scan as part of any CI/CD pipeline. The
action:

1. Builds the `nyxproxy-scan` CLI (cached between runs).
2. Crawls a target URL with the NyxProxy spider.
3. Passively scans every captured response with the same rule engine that
   the desktop app uses.
4. Writes SARIF 2.1.0, JSON, and HTML reports.
5. Fails the job if any finding meets or exceeds the `fail-on` severity
   threshold.

This closes the "Burp Pro ships a headless scanner runner; ours exists at
the crate level but no first-class CI action yet" gap from the comparison
matrix.

## Usage

```yaml
name: Security scan
on:
  pull_request:
  schedule:
    - cron: "0 4 * * *"

permissions:
  contents: read
  security-events: write   # required to upload SARIF

jobs:
  scan:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run NyxProxy scan
        id: nyx
        uses: gitboyabhayt/nyxproxy/action@main
        with:
          target: https://staging.example.com
          fail-on: medium
          max-urls: 500

      - name: Upload SARIF to code scanning
        if: always()
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: ${{ steps.nyx.outputs.sarif }}

      - name: Upload HTML report as artifact
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: nyxproxy-report
          path: |
            ${{ steps.nyx.outputs.html }}
            ${{ steps.nyx.outputs.json }}
```

## Inputs

| Input         | Default                 | Description                                               |
|---------------|-------------------------|-----------------------------------------------------------|
| `target`      | (required)              | Seed URL to scan.                                         |
| `scope`       | host of target          | Comma-separated allowed host substrings.                  |
| `fail-on`     | `high`                  | `info` \| `low` \| `medium` \| `high` \| `critical`.      |
| `max-urls`    | `200`                   | Cap total URLs visited.                                   |
| `max-depth`   | `3`                     | Cap crawl depth from seed.                                |
| `concurrency` | `4`                     | Parallel requests.                                        |
| `insecure`    | `false`                 | Set `true` to ignore TLS errors (staging only).           |
| `sarif-path`  | `nyxproxy-scan.sarif`   | SARIF output path (empty = skip).                         |
| `json-path`   | `nyxproxy-scan.json`    | JSON output path (empty = skip).                          |
| `html-path`   | `nyxproxy-scan.html`    | HTML output path (empty = skip).                          |
| `ref`         | `main`                  | NyxProxy ref to build the scanner from.                   |

## Outputs

| Output | Description                                       |
|--------|---------------------------------------------------|
| `sarif`| Path to the SARIF file (for code scanning).      |
| `json` | Path to the full JSON report.                    |
| `html` | Path to the human-readable HTML report.          |
| `failed`| `true` if findings above `fail-on` were produced. |

## Local equivalent

The same binary the action uses is `cargo install --path crates/nyxproxy-scan`
from `apps/desktop`, then:

```bash
nyxproxy-scan --target https://staging.example.com \
  --fail-on medium \
  --output-sarif report.sarif \
  --output-html report.html
```

Exit codes:

* `0` — clean (no findings above `--fail-on`).
* `1` — at least one finding above `--fail-on`.
* `2` — internal error.

## How it differs from Burp Pro CLI

| | NyxProxy action | Burp Pro CLI |
|---|---|---|
| License | Open source (MIT) | Per-seat commercial |
| SARIF for GH code scanning | Native | Requires conversion |
| Build/cache | Composite action, cached in `~/.cache/nyxproxy/bin` | Bring your own binary |
| Cost | Free | $474/user/year |
