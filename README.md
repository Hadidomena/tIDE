# tIDE

tIDE is an integrated development environment for the Tidal language.

**Status:** editor with Tidal highlighting, live GHCi session, SuperDirt/headless engine
orchestration, toolchain detection + diagnostics, project open/save. Sample library, transforms
and pattern analysis land in later milestones.

## Prerequisites

**To actually run Tidal code (detected and diagnosed by the app)**

1. **GHC + cabal** via [ghcup](https://www.haskell.org/ghcup/), then:
   ```sh
   cabal update && cabal install --lib tidal
   ```
   The `--lib` flag is required: plain `cabal install` registers no library for GHCi.
   Note: `ghc-pkg list` will not show this install (cabal uses a GHC environment file) — tIDE's
   detector accounts for that. The build needs a C++ toolchain (Xcode Command Line Tools on macOS,
   `build-essential` on Linux);
   Verify: `echo ':q' | ghci -package tidal -e 'import Sound.Tidal.Context'`
2. **SuperCollider + SuperDirt** (optional — without it tIDE runs in _headless_ mode: evaluation
   works, no sound):
   ```sh
   brew install --cask supercollider      # Linux: distribution package
   ```
   Then in the SuperCollider IDE install the **SuperDirt** quark (Quarks menu). It downloads the
   Dirt-Samples pack (~400 MB). tIDE starts/stops SuperCollider itself; you don't need to run the
   IDE while using tIDE.

Open the **Diagnostics** pane in the app to see exactly what is detected and what is yet to install.

## User Notes

### Keybinds

| Shortcut                                 | Action                                     |
| ---------------------------------------- | ------------------------------------------ |
| `Cmd/Ctrl+Enter`                         | evaluate the current line or selection     |
| `Cmd/Ctrl+Shift+Enter`                   | run the whole file, statement by statement |
| `Cmd/Ctrl+.`                             | hush — silence all running patterns        |
| `Cmd/Ctrl+S`                             | save                                       |
| `Cmd/Ctrl+Shift+S`                       | save as                                    |
| `Cmd/Ctrl+O`                             | open a file                                |
| `Tab` / `Shift+Tab`                      | accept autocomplete (or indent) / outdent  |
| `Enter` in the console row               | evaluate the typed line                    |
| `Cmd/Ctrl+Z`, `Shift+Cmd/Ctrl+Z`         | undo / redo (CodeMirror defaults)          |
| `Cmd/Ctrl+F`, `Cmd/Ctrl+A`, `Cmd/Ctrl+D` | find, select all, select next occurrence   |

### Additional instruments

- `chip`
- `chipbass`
- `chipled`
- `chipnoise`

**Example of usage:**

- `d1 $ s "chipbass" # note "c2 eb2 g2 bb2" # legato 0.8`

### Boot Aliases

The **logs** panel (GHCi + engine output) is hidden by default — toggle it with the `logs` button.
Audio input/output devices can be selected in **Diagnostics** (the list comes from SuperCollider
itself). Changing them applies after restarting the session; an input device is optional and
disabled by default (avoids sample-rate mismatches with a 16 kHz mic).

The full set of Tidal boot aliases is available: `setcps`, `setbpm`, `getcps`, `getbpm`, `solo`,
`unsolo`, `mute`, `unmute`, `once`, `first`, `asap`, `all`, `nudgeAll`, `resetCycles`, `hush`,
`panic`, and `d1`…`d16` (each `dN` uses orbit N−1).

## Quickstart (development)

**Build tooling (dev only)**

- **Node.js (LTS)** — via [mise](https://mise.jdx.dev): `mise use -g node@lts`
- **Rust** (stable) — `rustup`
- **Lefthook** — `brew install lefthook` (macOS); git hooks are mandatory in this repo

```sh
git clone https://github.com/Hadidomena/tIDE.git
cd tIDE
lefthook install          # one-time; installs pre-commit / commit-msg / pre-push hooks
npm install
npm run tauri dev         # opens the app window (use the native window)
```

To reproduce a file's state exactly, hit **restart** first
(fresh prelude, no leftover bindings) and then run the file. The engine is selected in
Diagnostics: `auto` (SuperDirt when detected, otherwise headless), or explicitly
`superdirt` / `headless`. Useful scripts: `npm run lint` (ESLint), `npm run format` (Prettier), `npm run check`
(svelte-check), and `cd src-tauri && cargo test` / `cargo clippy --all-targets -- -D warnings`.

## Git hooks (lefthook)

Commits are gated automatically (`lefthook.yml`):

- **pre-commit** — `git diff --check`, `cargo fmt` + `cargo clippy -D warnings` (staged `src-tauri/**`),
  `eslint --fix` + `prettier --write` (staged `src/**`), with auto-staging of fixes
- **pre-push** — `cargo test` + `npm run check`
- **post-merge** — `npm install` / `cargo fetch` after pulls

## Releases

Pushing a `v*` tag triggers the GitHub Actions matrix (`.github/workflows/build.yml`), which builds
installers for Windows, macOS (universal) and Linux and attaches them to a draft GitHub Release.
