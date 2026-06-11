#!/usr/bin/env bash
# Build the iOS app. Run from the repo root.
#
#   scripts/ios-build.sh sim     # simulator (debug) — pipeline smoke test
#   scripts/ios-build.sh device  # ad-hoc device (release) — needs signing
#
# GOTCHA (the one that bites first): the iOS std targets are managed by
# *rustup*, but Homebrew also ships a `cargo` that shadows it on PATH and has
# NO iOS std — the build then fails with "can't find crate for `core`". So we
# force rustup's cargo to the front of PATH here.
set -euo pipefail

export PATH="$HOME/.cargo/bin:$PATH"
cargo --version | grep -qi homebrew && {
  echo "error: still using Homebrew cargo — rustup cargo not found at ~/.cargo/bin" >&2
  exit 1
}

# One-time per checkout (idempotent): generates src-tauri/gen/apple (gitignored).
[ -d src-tauri/gen/apple ] || cargo tauri ios init

case "${1:-sim}" in
  sim)
    cargo tauri ios build --target aarch64-sim --debug
    echo "→ src-tauri/gen/apple/build/arm64-sim/BWOC Control Center.app"
    ;;
  device)
    # Ad-hoc: set the signing team + provisioning profile in Xcode first
    # (open src-tauri/gen/apple/bwoc-control-center.xcodeproj), or pass
    # TAURI_APPLE_DEVELOPMENT_TEAM. Produces a .ipa for the registered UDIDs.
    cargo tauri ios build
    ;;
  *)
    echo "usage: $0 [sim|device]" >&2; exit 2 ;;
esac
