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

🚧 **Design phase** — stack chosen (Tauri 2 + React 19 + Vite + shadcn/Tailwind). Code scaffold
pending design sign-off.

## Stack

- **Tauri 2** — desktop + iOS + Android from one Rust core; links `bwoc-core` directly, reuses the
  ed25519 signing crate. iOS builds ship via the self-hosted runner → OTA on bemindlabs.
- **React 19 + Vite + shadcn/ui (Tailwind v4)** — frontend (components borrowed from `bwoc-party`).

## Quality gate

Every PR runs CI (`.github/workflows/ci.yml`): Rust `fmt` + `clippy -D warnings` + `test`, and
frontend `typecheck` + `lint` + `build`. `main` is protected — PR + green checks required.
