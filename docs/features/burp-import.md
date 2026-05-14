# Import from Burp Suite (Feature E)

Status: **Shipped** — `Project options → Import from Burp Suite`.

## What it does

Many existing Burp Suite users have years of project data captured. NyxProxy
can now ingest those captures directly so users don't have to start from
scratch when migrating.

The importer reads Burp Suite's "Save items" XML format — produced by
`Proxy → HTTP history → select items → right-click → Save items… → Items
file (XML, base64-encoded)`. Every `<item>` is converted into a NyxProxy
`HttpFlow` and appended to the live history store, tagged with
`import:burp`.

## How to export from Burp Suite

1. Open Burp Suite Professional or Community.
2. Go to **Proxy → HTTP history**.
3. Select the rows you want to migrate (Ctrl/Cmd-A for everything).
4. Right-click → **Save items…**.
5. Format: **XML**. Tick **Base64-encode requests and responses** (default).
6. Save to a `.xml` file.

## How to import into NyxProxy

1. Open NyxProxy.
2. Sidebar → **Project options**.
3. Scroll to the **Import from Burp Suite** panel.
4. Click **Choose Burp XML…** and pick the `.xml` file.
5. The page shows a summary banner: how many items were seen, imported, and
   skipped, plus the Burp version embedded in the file.
6. The imported flows show up immediately in **Logger / History**, tagged
   with `import:burp` so you can filter or scope-include them later.

## What's preserved per item

| Burp field           | NyxProxy field                         |
|----------------------|----------------------------------------|
| `<time>`             | `HttpFlow.started_at` (best-effort)    |
| `<url>`              | `request.url`                          |
| `<host>`/`<port>`/`<protocol>` | `request.scheme` / `request.authority` |
| `<method>` / raw request line | `request.method`                |
| `<path>` / raw request line   | `request.path`                  |
| `<request>` raw bytes | parsed into headers + `body_b64`     |
| `<status>`           | `response.status`                      |
| `<response>` raw bytes | parsed into headers + `body_b64`    |
| `<responselength>`   | `response.body_size`                   |
| `<comment>`          | added as `comment:<text>` tag          |

The raw `<request>` and `<response>` bodies (base64-decoded if the
`base64="true"` attribute is set) are re-parsed as HTTP/1.1 messages
using a tolerant tokenizer (LF and CRLF both accepted) so the headers
panel in NyxProxy is identical to what Burp captured on the wire.

## Robustness

* Items missing a required field (e.g. `<url>`) are **skipped**, not
  rejected — the summary reports the count and the first error message.
* Malformed HTTP framing in `<request>` / `<response>` is handled by a
  best-effort tokenizer rather than aborting the whole import.
* The XML stream parser is `quick-xml`, which is incremental, so we can
  ingest multi-GB Burp histories without loading the whole file into
  memory.

## Tests

`apps/desktop/crates/nyxproxy-core/src/burp_import.rs` ships with four
unit tests, all passing:

* `parses_minimal_burp_item` — single item with CRLF framing, asserts
  method, path, version, headers, response status, and the
  `import:burp` + `comment:…` tags.
* `imports_multiple_items_and_reports_summary` — three POST items,
  asserts every flow is captured correctly and summary counts match.
* `handles_lf_only_line_separators_in_request_body` — LF-only line
  endings (some Burp exports normalise this) still parse.
* `skips_malformed_item_but_keeps_good_one` — one item missing `<url>`
  is skipped, the next valid item is imported, summary reports the
  skipped count + error message.

```
$ cargo test -p nyxproxy-core --lib burp_import
test result: ok. 4 passed; 0 failed
```

## Out of scope (for now)

* Importing Burp's binary `.burp` project files — Burp's serialised
  Java format is closed and not officially documented. Burp users
  wanting to migrate their full project should export to XML first
  (Burp does this losslessly).
* Importing Burp's **Sitemap** and **Issues** trees. These are stored
  separately from the HTTP history; we may add a second importer for
  them in a later release.
* Importing scope rules. Burp's scope is expressed as host/path/port
  include/exclude patterns; mapping those to NyxProxy's
  `scope_include` / `scope_exclude` substring lists is straightforward
  but not done yet.
