# JWT toolkit

The JWT toolkit lives in **Decoder → JWT tab**. It's a fully offline, pure-Rust implementation — no remote calls, no external libraries beyond what is already in the desktop bundle.

## What it does

| Action | Description |
|---|---|
| **Decode** | Parses `header.payload.signature` into structured JSON. Rejects malformed input (not exactly three segments, non-base64url, non-JSON header/payload). |
| **Analyse** | Runs static checks against the header and payload, emitting findings (see below). |
| **Re-sign HS256** | Takes the current header + payload and re-signs with a user-supplied secret using HMAC-SHA-256. Useful to validate the secret guessed by *Brute force*. |
| **alg=none** | Strips the signature and rewrites the header to `{"alg":"none","typ":"JWT"}`. Lets you confirm whether a target naïvely accepts an unsigned token. |
| **Brute force HS256** | Iterates a wordlist (one secret per line) and reports the first secret that verifies the existing signature, plus the elapsed time and number of candidates tried. |

## Findings emitted by *Analyse*

| Kind | Severity | When emitted |
|---|---|---|
| `alg_none` | high | `header.alg` is literally `"none"`. |
| `weak_algorithm` | medium | `header.alg` is one of `HS256`, `none`, `RS1`, `MD5`. |
| `missing_exp` | medium | Payload has no `exp` claim. |
| `expired_token` | low | `exp` is in the past. |
| `long_lived_token` | low | `exp - iat > 24h` (sliding tokens with no rotation). |
| `kid_injection` | medium | `kid` contains `..`, `/`, `\`, `'`, `"`, `;` (path traversal / SQLi hint). |
| `jku_jwk_header` | medium | `jku` or `jwk` present (untrusted key sources). |
| `rsa_hmac_confusion` | high | `header.alg = HS256` but `kid` looks like an RSA public-key reference. |

## Where the implementation lives

- Rust core: <ref_file file="/home/ubuntu/nyxproxy/apps/desktop/crates/nyxproxy-core/src/jwt.rs" /> — 8 unit tests in the same file.
- Tauri commands: `jwt_decode_cmd`, `jwt_analyze_cmd`, `jwt_encode_hs256_cmd`, `jwt_encode_none_cmd`, `jwt_brute_hs256_cmd`.
- React UI: `src/pages/Decoder.tsx` (`JwtTab` component).
- TypeScript wrapper: `JwtApi` in `src/tauri/api.ts`.

## Worked example — confirming a weak secret

1. Paste any HS256 token into the input — try the RFC 7519 sample token (pre-filled).
2. Click **Decode & analyse**. The right pane shows decoded header/payload, signature, and a list of findings.
3. Expand **Brute-force HS256 secret**. The textarea is pre-filled with the OWASP top-10 weak secrets — leave as-is or paste your own.
4. Click **Run brute force**. Within milliseconds you'll see `secret = your-256-bit-secret · tried = 5 · elapsed = 1ms`.
5. Type the secret into **HS256 secret**, edit the payload (e.g. flip `"role":"user"` to `"admin"`), and click **Re-sign HS256**. The textarea is repopulated with the new token, ready to paste into Repeater.
