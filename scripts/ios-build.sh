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
    # Signed build for registered devices → a .ipa for OTA install.
    #
    # tauri 2.11 does NOT honor bundle.iOS.developmentTeam, so inject the team
    # into the xcodegen source (project.yml) after init and regenerate — this
    # survives the build (tauri reuses the existing project.yml). Set BWOC_IOS_TEAM
    # (default = the bemindlabs team). The signing identity + provisioning
    # profile are resolved automatically from the keychain.
    TEAM="${BWOC_IOS_TEAM:-944QFC9G5N}"
    YML="src-tauri/gen/apple/project.yml"
    if ! grep -q "DEVELOPMENT_TEAM: ${TEAM}" "$YML" 2>/dev/null; then
      perl -0pi -e "s/(PRODUCT_BUNDLE_IDENTIFIER: \S+)/\$1\n      DEVELOPMENT_TEAM: ${TEAM}\n      CODE_SIGN_STYLE: Automatic/" "$YML"
      ( cd src-tauri/gen/apple && xcodegen generate >/dev/null )
    fi
    # debugging = development distribution (installs on the profile's devices).
    cargo tauri ios build --export-method debugging --ci
    echo "→ src-tauri/gen/apple/build/arm64/BWOC Control Center.ipa"
    echo "  next: BASE_URL=https://<host>.<tailnet>.ts.net:10131 scripts/make-ota.sh"
    ;;
  *)
    echo "usage: $0 [sim|device]" >&2; exit 2 ;;
esac
