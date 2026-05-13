# NyxProxy plugins

NyxProxy supports out-of-process plugins. Each plugin lives in its own
sub-directory under `~/.nyxproxy/plugins/` (the desktop app reads from
this location at startup) and is described by a `plugin.json` manifest:

```json
{
  "id": "your-plugin-id",
  "name": "Human-readable name",
  "version": "0.1.0",
  "description": "What this plugin does.",
  "author": "you",
  "command": ["python3", "main.py"],
  "capabilities": ["scan_flow"]
}
```

The host process spawns the plugin on demand and writes a single
newline-delimited JSON-RPC 2.0 request to stdin. The plugin must respond
with a single line of JSON to stdout, then exit. This keeps the host /
plugin contract simple and makes plugin crashes survivable.

## Supported capabilities

| Capability  | Request method | Params                                  | Response                                                                                          |
|-------------|----------------|-----------------------------------------|---------------------------------------------------------------------------------------------------|
| `scan_flow` | `scan_flow`    | `{ "flow": <HttpFlow JSON> }`           | `{ "issues": [<Issue JSON>, …] }`                                                                  |

`HttpFlow` and `Issue` schemas are defined in `nyxproxy-core` and exposed
verbatim over the JSON-RPC wire — see `apps/desktop/crates/nyxproxy-core/src/model.rs`
and `…/scanner.rs`.

## Example plugin

`example-wordpress/` is a reference Python plugin. To install it, copy the
folder into `~/.nyxproxy/plugins/` and click **Reload** in the Extender
page.

## Testing locally

```
echo '{"jsonrpc":"2.0","id":1,"method":"scan_flow","params":{"flow":{"id":"a","request":{"method":"GET","path":"/wp-login.php","authority":"example.com","headers":[],"body_b64":""}}}}' \
  | python3 apps/desktop/plugins/example-wordpress/main.py
```
