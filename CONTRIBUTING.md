# Contributing to NyxProxy

Thanks for considering a contribution! NyxProxy is open-source under the MIT license and contributions from everyone are welcome — security researchers, frontend devs, Rust enthusiasts, technical writers, designers.

Before opening a PR, please read this short guide.

## Code of Conduct

By participating in this project you agree to abide by the [Code of Conduct](CODE_OF_CONDUCT.md).

## Project layout

```
nyxproxy/
├── apps/
│   ├── backend/             # FastAPI AI gateway (Python 3.11+)
│   └── desktop/             # Tauri desktop app
│       ├── src/             # React + TypeScript frontend
│       ├── src-tauri/       # Tauri integration (Rust)
│       └── crates/
│           └── nyxproxy-core/  # Pure-Rust proxy, scanner, intruder, etc.
├── docs/                    # User & developer documentation
├── .github/workflows/       # CI pipelines
└── render.yaml              # Hosted backend deployment manifest
```

## Development setup

### Backend (Python)

```bash
cd apps/backend
python -m venv .venv && source .venv/bin/activate
pip install -e ".[dev]"
ruff check . && ruff format --check . && pytest -q
uvicorn nyxproxy_backend.main:app --reload
```

### Desktop (Rust + TypeScript)

Requires: Rust ≥ 1.85, Node ≥ 18, and (Linux only)

```bash
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev libssl-dev pkg-config
```

Then:

```bash
cd apps/desktop
npm install
cargo test -p nyxproxy-core --release
npx tsc --noEmit
npm run tauri dev
```

## Coding standards

* Rust: `cargo fmt` + `cargo clippy --workspace --all-targets`
* TypeScript: `npx tsc --noEmit` must pass; prefer named exports, no `any`
* Python: `ruff check . && ruff format --check .`
* Add unit tests for every public function in `nyxproxy-core`
* Keep frontend components ≤ 300 lines — extract sub-components when bigger
* Don't add dependencies without explaining why in the PR description

## Commit & branch conventions

* Branch from `main` with the shape `devin/<ts>-<slug>` or `<your-name>/<slug>`
* Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`
* Squash-merge is fine; keep history readable

## Pull request checklist

- [ ] Tests added / updated and passing locally
- [ ] `cargo test -p nyxproxy-core --release` green
- [ ] `cd apps/desktop && npx tsc --noEmit && npx vite build` clean
- [ ] `cd apps/backend && ruff check . && ruff format --check . && pytest -q` clean
- [ ] Documentation updated (`docs/features/<your-feature>.md` for new features)
- [ ] No secrets committed
- [ ] PR title follows Conventional Commits
- [ ] Linked any related issues

## Reporting bugs

Open an issue with: NyxProxy version, OS, reproduction steps, screenshots, and the relevant snippet from `~/.nyxproxy/logs/` if any.

## Reporting security issues

See [SECURITY.md](SECURITY.md). Do **not** use the public issue tracker for vulnerabilities.

## License

By contributing you agree that your contributions will be licensed under the MIT license, same as the rest of the project.
