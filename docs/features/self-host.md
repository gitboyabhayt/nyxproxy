# Self-hosting wizard (Feature Y)

Generate a complete Docker deployment bundle for the NyxProxy backend with
optional reverse-proxy and tunnel sidecars.

## Output bundle

The wizard renders five files into a directory you choose:

| File              | Purpose                                                                  |
| ----------------- | ------------------------------------------------------------------------ |
| `Dockerfile`      | Python 3.12 slim image. Installs `apps/backend` and runs `uvicorn`.       |
| `docker-compose.yml` | Service definitions: `backend`, optional `caddy`, optional `cloudflared`. |
| `.env.example`    | Required environment variables (copy to `.env` before `up`).              |
| `Caddyfile`       | Only when **Add Caddy** is checked — auto-TLS reverse proxy config.       |
| `README.md`       | Boot instructions for the bundle.                                         |

## Options

- **Backend port** — host port the FastAPI server binds to (default `8080`).
- **Add Caddy reverse proxy with auto-TLS** — adds a Caddy sidecar that
  terminates TLS for the public host you specify. Caddy issues + renews Let's
  Encrypt certificates automatically.
- **Public host (for Caddy TLS)** — DNS name pointing at this server.
- **Add Cloudflare Tunnel sidecar** — adds `cloudflared` so you can expose
  the backend without opening any inbound ports. Requires a tunnel token in
  `.env` (`CLOUDFLARE_TUNNEL_TOKEN`).
- **Persistent data volume** — when enabled, mounts a Docker volume at
  `/data` so logs, cached LLM responses, and SQLite state survive container
  rebuilds.

## Boot flow

```bash
cp .env.example .env       # fill in keys
docker compose pull
docker compose up -d
docker compose logs -f backend
```

If Caddy is enabled, point your DNS A/AAAA record at the server before
boot — Caddy will issue a certificate on first launch and refuse to start
until DNS resolves.

## Tests

`apps/desktop/crates/nyxproxy-core/src/selfhost.rs` ships five unit tests
that cover: default bundle layout, custom port propagation, Caddy emission,
Cloudflare Tunnel sidecar, persistent volume omission.

## Related Tauri commands

- `selfhost_render_cmd` — returns the rendered bundle as strings.
- `selfhost_write_cmd` — writes the bundle to a directory on disk.
