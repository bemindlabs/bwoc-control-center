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

🚧 **Phase 1 scaffold** — design closed (12 decisions, see [DESIGN.md](DESIGN.md)). The app renders
a live local fleet (LocalShell), and `bwocd` serves the signed read API end-to-end. Next:
RemoteHttp transport in the app, QR + SSO enrollment, iOS ad-hoc build.

## Layout

| Path | What |
|---|---|
| `src/` + `src-tauri/` | Tauri 2 app — React 19 UI + Rust core (`fleet_snapshot`, `agent_inbox`, `host_info` over the **LocalShell** transport) |
| `bwocd/` | Control daemon (axum) — wraps `bwoc … --json` behind ed25519-signed HTTP, loopback-only + `tailscale serve` |

```bash
pnpm install && pnpm tauri dev        # desktop app against the local workspace
cargo run --manifest-path bwocd/Cargo.toml -- --workspace ~/workspaces/bwoc   # daemon on :10130
```

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
