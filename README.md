<div align="center">

# 🎛️ bwoc-control-center

**Cross-platform app to manage & configure BWOC agents and infra — desktop + mobile, one codebase.**

![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB)
![React](https://img.shields.io/badge/React-19-149eca)
![Vite](https://img.shields.io/badge/Vite-7-646CFF)
![Rust](https://img.shields.io/badge/Rust-2021-orange)

</div>

## What it is

A single Tauri 2 app (Rust core + React/Vite web UI) that runs on **macOS / Windows / Linux**
and **iOS / Android** to manage the BWOC ecosystem: agents, gateway/relay, configs, Ollama
models, party feed, devices.

- **Desktop (local mode):** the Rust core shells `bwoc --json` directly — no network.
- **Mobile (remote mode):** reaches a control surface over the tailnet, authed with the agent's
  **ed25519 identity** (same pattern as `bwoc-party`).

See **[DESIGN.md](DESIGN.md)** for the full architecture, API surface, and MVP plan.

## Status

🚧 **Phase 1** — design closed (12 decisions, see [DESIGN.md](DESIGN.md)). Desktop app reads local
(LocalShell) **and** remote hosts (RemoteHttp, signed) with a host switcher; `bwocd` serves the
signed read API and is deployed on anna + busaba; the iOS app builds and runs on the simulator.
Next: enable `tailscale serve`, QR + SSO enrollment, iOS ad-hoc device build.

## Layout

| Path | What |
|---|---|
| `src/` + `src-tauri/` | Tauri 2 app — React 19 UI + Rust core. `Transport` resolves per call: `local` → LocalShell (`bwoc --json`), a host name → RemoteHttp (signed) |
| `cc-signing/` | Shared ed25519 wire primitives — imported by BOTH the app (signer) and bwocd (verifier) so the format can't drift |
| `bwocd/` | Control daemon (axum) — wraps `bwoc … --json` behind signed HTTP, loopback-only + `tailscale serve` |
| `deploy/` + `scripts/` | launchd/systemd units, tailnet guide, `ios-build.sh` |

```bash
pnpm install && pnpm tauri dev        # desktop app against the local workspace
cargo run --manifest-path bwocd/Cargo.toml -- --workspace ~/workspaces/bwoc   # daemon on :10130
scripts/ios-build.sh sim              # build + the iOS app for the simulator
pnpm tauri build && scripts/make-dmg.sh   # release .app + a distributable .dmg
```

> **macOS packaging gotcha:** `tauri build`'s built-in DMG step drives Finder via
> AppleScript and fails in any headless/automation context. `scripts/make-dmg.sh`
> builds the same `.dmg` with `hdiutil` only (no Finder) — the `.app` from
> `tauri build` is always fine; only its DMG layout step is the problem.
>
## iOS over-the-air install

The app builds a **signed** `.ipa` for the team's registered devices and serves it
OTA over the tailnet (no public hosting — iOS just needs HTTPS, which `tailscale
serve` provides):

```bash
scripts/ios-build.sh device                                   # signed .ipa
BASE_URL=https://<host>.<tailnet>.ts.net:10131 scripts/make-ota.sh   # manifest + page → out/ota
# serve out/ota over HTTPS (macOS: a loopback static server + `tailscale serve`):
python3 -m http.server 18131 --bind 127.0.0.1 --directory out/ota &
tailscale serve --bg --https=10131 http://127.0.0.1:18131
```

Then open `https://<host>.<tailnet>.ts.net:10131/` in **Safari** on a device that is
(a) one of the provisioning profile's registered devices and (b) on the tailnet, and
tap **Install**. The dev profile here covers 4 devices, valid until 2027-04.

**Persistent hosting** (survives reboot): stage the bundle into a stable dir and run
the static server under launchd — the `tailscale serve` mapping persists on its own.

```bash
mkdir -p ~/.local/share/bwoc-ota && cp out/ota/* ~/.local/share/bwoc-ota/
sed "s|__HOME__|$HOME|g" deploy/tech.bemind.bwoc-ota.plist \
  > ~/Library/LaunchAgents/tech.bemind.bwoc-ota.plist
launchctl load ~/Library/LaunchAgents/tech.bemind.bwoc-ota.plist
# to publish a new build later: rebuild → make-ota → re-copy into ~/.local/share/bwoc-ota
```

> **`bwoc` discovery:** the installed app resolves the `bwoc` binary from the
> well-known install dirs (`/opt/homebrew/bin`, `~/.cargo/bin`, …) because a
> Finder-launched app has a minimal PATH. Override with `BWOC_BIN`.

> **iOS build gotcha:** the iOS Rust std targets are managed by *rustup*, but Homebrew's `cargo`
> shadows it on PATH with no iOS std (`can't find crate for core`). `scripts/ios-build.sh` forces
> `~/.cargo/bin` to the front of PATH — do the same in any CI/build-server invocation.

`bwocd` auth is wire-compatible with `bwoc-party`'s signed requests (same `X-BWOC-*` headers +
canonical payload), so party's `sign.mjs`/`partyctl` double as test clients. Controllers register
in `<workspace>/.bwoc/bwocd/controllers.toml` (`[[controller]] id/public_key/scope`).

## Stack

- **Tauri 2** — desktop + iOS + Android from one Rust core. iOS builds ship via the self-hosted
  runner → OTA on bemindlabs.
- **React 19 + Vite + Tailwind v4** — frontend (shadcn/ui lands with the host-list UI).
- **bwocd** — axum + ed25519-dalek; the CLI stays the single source of truth (no logic forked).

## Quality gate

Every PR runs CI (`.github/workflows/ci.yml`): Rust `fmt` + `clippy -D warnings` + `test`, and
frontend `typecheck` + `lint` + `build`. `main` is protected — PR + green checks required.
