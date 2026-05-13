# NyxProxy backend

A small FastAPI service that fronts multiple free AI providers behind a single, OpenAI-compatible API. The NyxProxy desktop app talks to this gateway for every AI feature (explain request, find vulnerabilities, generate fuzzing payloads, free-form chat).

## Why a backend?

- **Keys stay on the server.** End users do not need to sign up for 9 different AI providers.
- **Fallback routing.** If one provider is rate-limited, the gateway can try the next configured one.
- **Uniform schema.** The desktop app speaks one API regardless of which provider answered.

## Providers supported

| Provider | Env var(s) | Get a free key |
|---|---|---|
| Groq | `GROQ_API_KEY` | https://console.groq.com/keys |
| OpenRouter | `OPENROUTER_API_KEY` | https://openrouter.ai/keys |
| HuggingFace | `HF_TOKEN` | https://huggingface.co/settings/tokens |
| Cloudflare Workers AI | `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` | https://dash.cloudflare.com/profile/api-tokens |
| GitHub Models | `GITHUB_MODELS_TOKEN` | https://github.com/settings/tokens (scope: `models:read`) |
| NVIDIA NIM | `NVIDIA_API_KEY` | https://build.nvidia.com |
| Bytez | `BYTEZ_API_KEY` | https://bytez.com/dashboard/keys |
| Gemini | `GEMINI_API_KEY` | https://aistudio.google.com/apikey |
| Ollama (local) | `OLLAMA_BASE_URL` | https://ollama.com |

## Run locally

```bash
python -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"
cp .env.example .env  # add at least one provider key
uvicorn nyxproxy_backend.main:app --reload --port 8000
```

Test it:

```bash
curl http://localhost:8000/v1/providers
curl -X POST http://localhost:8000/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"messages":[{"role":"user","content":"hi"}]}'
```

## Deploy to Render

The repo ships a `render.yaml` blueprint. From the Render dashboard: **New → Blueprint → connect this repo**, then paste your provider keys into the service's Environment tab.
