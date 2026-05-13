# NyxProxy

> AI-driven, blazing-fast, open-source alternative to Burp Suite. Built in Rust + Tauri for Windows and Linux.

NyxProxy is an intercepting HTTPS proxy and web-app security testing suite, designed from the ground up to be **faster, lighter, and smarter** than Burp Suite. The proxy core is written in **Rust** (no JVM, low memory footprint), the GUI is a **native Tauri app** with a React + TypeScript frontend, and every tool is deeply integrated with AI to accelerate the security workflow.

Free AI providers are wired through a hosted backend gateway (deployed on Render) so users do not need their own keys to get started.

## Phase 1 features (this release)

- **Intercepting HTTPS proxy** with on-the-fly CA cert generation (HTTP/1.1, TLS via `rustls`).
- **Request history** with live updates via Tauri events.
- **Inspector** for headers, body (text, JSON, hex), URL params, cookies.
- **Repeater** — clone any captured request, edit, resend, diff response.
- **AI Assistant** — explain a request, find likely vulnerabilities, generate fuzzing payloads. Powered by Groq, OpenRouter, HuggingFace, Cloudflare Workers AI, GitHub Models, NVIDIA NIM, Bytez, Gemini, and Ollama (local) via a pluggable backend gateway.
- **Burp-style dark UI** with a sidebar of tools, fully responsive split panels.
- **Cross-platform**: Windows and Linux desktop builds via GitHub Actions.

## Roadmap

| Phase | Scope |
|------|--------|
| 1 (now) | Proxy core, GUI shell, Repeater, AI Assistant, multi-provider backend |
| 2 | Intruder (sniper/clusterbomb/pitchfork/battering-ram), Decoder, Comparer, Search/filter |
| 3 | Passive scanner, AI-powered active scanner, WebSocket viewer, HTTP/2 |
| 4 | Spider/Crawler, Sequencer, Collaborator-style OOB server, plugin API (Python + JS) |
| 5 | Recorded logins, macros, session handling, advanced reporting, team sync |

## Repository layout

```
apps/
  backend/       FastAPI AI gateway (deployed on Render)
  desktop/       Tauri + React + Rust proxy core
.github/
  workflows/     CI for backend and desktop builds
render.yaml      One-click Render deployment blueprint
```

## Quick start (development)

### Backend (AI gateway)

```bash
cd apps/backend
python -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"
cp .env.example .env  # fill in any provider API keys you have
uvicorn nyxproxy_backend.main:app --reload --port 8000
```

The backend speaks an OpenAI-compatible `/v1/chat/completions` schema plus higher-level endpoints `/v1/analyze/request`, `/v1/analyze/vulns`, `/v1/analyze/payloads`. It transparently routes to whichever provider you configured.

### Desktop app

```bash
cd apps/desktop
npm install
npm run tauri dev
```

On first run, NyxProxy generates a CA certificate at `~/.nyxproxy/ca/`. Install/trust this cert in your browser to intercept HTTPS.

## Deploying the backend to Render

1. Fork or push this repo to your GitHub.
2. In Render, click **New → Blueprint** and point at this repo. Render reads `render.yaml`.
3. After the service is created, open **Environment** in the Render dashboard and paste in whichever provider keys you have (`GROQ_API_KEY`, `OPENROUTER_API_KEY`, `HF_TOKEN`, `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID`, `GITHUB_MODELS_TOKEN`, `NVIDIA_API_KEY`, `BYTEZ_API_KEY`, `GEMINI_API_KEY`).
4. Set `NYXPROXY_BACKEND_URL` in `apps/desktop/.env` to your Render URL.

The backend is provider-agnostic — any one key is enough to start.

## Security

NyxProxy is a security testing tool. Only intercept traffic you are authorised to test. The generated CA private key never leaves the user's machine and is stored with restrictive permissions (`0600` on Linux, ACL'd on Windows).

## License

MIT — see [LICENSE](LICENSE).
