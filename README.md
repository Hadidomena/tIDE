# tIDE

tIDE is an integrated development environment for the Tidal language.

**Status:** M2 — editor with Tidal highlighting, live GHCi session, SuperDirt/headless engine
orchestration, toolchain detection + diagnostics, project open/save. Sample library, transforms
and pattern analysis land in later milestones.

## Prerequisites

**Build tooling**

- **Node.js (LTS)** — via [mise](https://mise.jdx.dev): `mise use -g node@lts`
- **Rust** (stable) — `rustup`
- **Lefthook** — `brew install lefthook` (macOS); git hooks are mandatory in this repo

**To actually run Tidal code (detected and diagnosed by the app)**

1. **GHC + cabal** via [ghcup](https://www.haskell.org/ghcup/), then:
   ```sh
   brew install cmake                      # tidal-link's native build needs it (apt: cmake)
   cabal update && cabal install --lib tidal
   ```
   The `--lib` flag is required: plain `cabal install` registers no library for GHCi.
   Note: `ghc-pkg list` will not show this install (cabal uses a GHC environment file) — tIDE's
   detector accounts for that.
   Verify: `echo ':q' | ghci -package tidal -e 'import Sound.Tidal.Context'`
2. **SuperCollider + SuperDirt** (optional — without it tIDE runs in _headless_ mode: evaluation
   works, no sound):
   ```sh
   brew install --cask supercollider      # Linux: distribution package
   ```
   Then in the SuperCollider IDE install the **SuperDirt** quark (Quarks menu). It downloads the
   Dirt-Samples pack (~400 MB). tIDE starts/stops SuperCollider itself; you don't need to run the
   IDE while using tIDE.

Open the **Diagnostics** pane in the app to see exactly what is detected and what to install.

## Quickstart (development)

```sh
git clone https://github.com/Hadidomena/tIDE.git
cd tIDE
lefthook install          # one-time; installs pre-commit / commit-msg / pre-push hooks
npm install
npm run tauri dev         # opens the app window (use the native window)
```

In the window: `Cmd/Ctrl+Enter` evaluates the current line or selection, `Cmd/Ctrl+Shift+Enter`
(or the **run file** button) replays the whole file statement by statement, `Cmd/Ctrl+.` (or the
hush button) silences all patterns. To reproduce a file's state exactly, hit **restart** first
(fresh prelude, no leftover bindings) and then run the file. The engine is selected in
Diagnostics: `auto` (SuperDirt when detected, otherwise headless), or explicitly
`superdirt` / `headless`.

Built-in additional instruments (no samples needed): `chip`, `chipbass`, `chipled`, `chipnoise` —
for example `d1 $ s "chipbass" # note "c2 eb2 g2 bb2" # legato 0.8`.
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
