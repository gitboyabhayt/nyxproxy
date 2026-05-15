# Recorded Playwright login macros (Feature B)

> Burp Pro lets you record a login flow inside its embedded Chromium and
> replay it from "Session handling rules → macros". NyxProxy reaches the
> same end state via the open-source
> [`npx playwright codegen`](https://playwright.dev/docs/codegen) tool: you
> record once with Playwright's official recorder, paste the generated
> spec file into NyxProxy, and we parse it into a structured DSL stored on
> disk.

## Why this design

Shipping a *built-in* recorder would force us to embed Chromium or a
Playwright dependency directly inside the Tauri shell — that adds ~120 MB
to the installer and reintroduces the JVM-style "fat runtime" we worked
hard to avoid. Instead we lean on Playwright's existing recorder, which:

- runs in the user's normal Chromium / Firefox / WebKit binary,
- already handles HTTPS, Shadow DOM, modern web auth (OAuth popups, etc.),
- produces a deterministic `.spec.ts` we can parse losslessly.

A second-pass run with `npx playwright test --headed --proxy-server=…`
replays the file with NyxProxy's listener configured as the upstream
HTTP/HTTPS proxy — every request lands in the History store and every
response is mitm'd by the existing CA, so the rest of the toolchain
(Repeater, Intruder, Scanner, AI) sees the requests exactly as if you'd
driven the browser by hand.

## Recording

```bash
# 1. one-time bootstrap in any working directory
npm install -D @playwright/test
npx playwright install

# 2. record. NyxProxy listens on 127.0.0.1:8080 by default.
npx playwright codegen \
  --target=javascript \
  --proxy-server=http://127.0.0.1:8080 \
  https://target.example.com/login

# 3. save the file Playwright opens (it auto-writes to the cwd as
#    `recording.spec.ts` once you click "Stop recording").
```

## Importing into NyxProxy

1. Open the **Macros** page.
2. Expand the **Browser-recorded macros (Playwright)** section at the top.
3. Click **Import codegen .spec.ts**.
4. Give the recording a name, paste the contents of the `.spec.ts` file,
   click **Import**.

NyxProxy parses the file into structured actions:

| Action | Source line example |
| --- | --- |
| `navigate` | `await page.goto('https://x.test');` |
| `click` | `await page.getByRole('button', { name: 'Sign in' }).click();` |
| `fill` | `await page.getByRole('textbox', { name: 'Email' }).fill('a@b.c');` |
| `press` | `await page.locator('#submit').press('Enter');` |
| `wait_for_url` | `await page.waitForURL('https://x.test/dashboard');` |
| `expect_url` | `await expect(page).toHaveURL('https://x.test/dashboard');` |
| `raw` | Any line we don't recognise (preserved verbatim, never silently dropped) |

The parsed recording is stored as JSON under
`~/.nyxproxy/playwright/<id>.json`. Existing macros (the request-chain
kind) and Playwright recordings live side-by-side without colliding.

## Replaying

Inside Tauri:

```bash
# Tauri menu → Macros → Recorded macros → select → "Replay"
```

When replay is invoked NyxProxy shells out to:

```bash
npx playwright test <id>.spec.ts \
  --reporter=line \
  --project=chromium \
  -c <generated playwright config>
```

with `playwright.config.js` set to:

```js
{ use: { proxy: { server: 'http://127.0.0.1:8080' }, ignoreHTTPSErrors: true } }
```

so the on-the-fly CA is trusted and every request is captured. The
returned exit code + stdout are surfaced in the macros panel.

If `npx playwright --version` fails on the host the UI shows an actionable
"Playwright not detected" badge with an `npm install` hint instead of a
generic spawn error.

## Files

| Layer | File |
| --- | --- |
| Rust core (parser + on-disk store + `detect_playwright`) | `apps/desktop/crates/nyxproxy-core/src/playwright.rs` |
| Tauri commands | `apps/desktop/src-tauri/src/commands.rs` (`playwright_*`) |
| Frontend API wrapper | `apps/desktop/src/tauri/api.ts` (`PlaywrightApi`) |
| Frontend types | `apps/desktop/src/tauri/types.ts` (`PlaywrightRecording`, `PlaywrightAction`, `PlaywrightAvailability`) |
| React UI | `apps/desktop/src/pages/Macros.tsx` (`PlaywrightRecordingsSection`) |

## Tests

- Parser: 5 unit tests covering navigate / fill / click / full login spec /
  unknown-line preservation.
- Store: 1 round-trip test (`save` → `list` → `get` → `delete`).
- Detection: 1 smoke test that `detect_playwright` never panics regardless
  of whether Playwright is installed.

Run with:

```bash
cargo test -p nyxproxy-core --lib playwright
```
