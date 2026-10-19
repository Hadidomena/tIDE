# tIDE

tIDE is an integrated development environment for the Tidal language.

**Status:** scaffold — Tauri v2 + Svelte 5 + TypeScript shell with a wired SQLite schema.
The editor/REPL engine, sample library, transforms and pattern analyzer land in future steps.

## Prerequisites

- **Node.js (LTS)** — via [mise](https://mise.jdx.dev): `mise use -g node@lts`
- **Rust** (stable) — `rustup`
- **Lefthook** — `brew install lefthook` (macOS); git hooks are mandatory in this repo
- For using tIDE itself: a working **GHC + Tidal** install, optionally **SuperCollider + SuperDirt**
  (the app detects these and diagnoses problems — see the Diagnostics pane once M2 lands)

## Quickstart (development)

```sh
git clone git@github.com:Hadidomena/tIDE.git
cd tIDE
lefthook install          # one-time; installs pre-commit / commit-msg / pre-push hooks
npm install
npm run tauri dev         # opens the app window (use the native window)
```

Useful scripts: `npm run lint` (ESLint), `npm run format` (Prettier), `npm run check`
(svelte-check), and `cd src-tauri && cargo test` / `cargo clippy --all-targets -- -D warnings`.

## Git hooks (lefthook)

Commits are gated automatically (`lefthook.yml`):

- **pre-commit** — `git diff --check`, `cargo fmt` + `cargo clippy -D warnings` (staged `src-tauri/**`),
  `eslint --fix` + `prettier --write` (staged `src/**`), with auto-staging of fixes
- **commit-msg** — Conventional Commits (`type(scope)?: subject`, e.g. `feat(editor): send eval command`)
- **pre-push** — `cargo test` + `npm run check`
- **post-merge** — `npm install` / `cargo fetch` after pulls

## Releases

Pushing a `v*` tag triggers the GitHub Actions matrix (`.github/workflows/build.yml`), which builds
installers for Windows, macOS (universal) and Linux and attaches them to a draft GitHub Release.
