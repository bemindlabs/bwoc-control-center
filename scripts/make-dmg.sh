#!/usr/bin/env bash
# Build a distributable .dmg from the release .app using hdiutil only.
#
# Why not `tauri build`'s dmg: it drives Finder via AppleScript to lay out the
# window, which fails in any headless / automation context (no Finder control
# permission). hdiutil needs no Finder, so this is reproducible on a build
# server. Run after `pnpm tauri build` (or it builds for you).
set -euo pipefail
cd "$(dirname "$0")/.."

APP="src-tauri/target/release/bundle/macos/BWOC Control Center.app"
if [ ! -d "$APP" ]; then
  echo "no release .app — building…" >&2
  pnpm tauri build || true   # dmg step may fail; the .app still builds
fi
[ -d "$APP" ] || { echo "error: $APP not found" >&2; exit 1; }

VERSION=$(/usr/libexec/PlistBuddy -c "Print CFBundleShortVersionString" "$APP/Contents/Info.plist")
OUT="src-tauri/target/release/bundle/dmg/BWOC-Control-Center_${VERSION}_aarch64.dmg"
STAGE=$(mktemp -d)
mkdir -p "$(dirname "$OUT")"

cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"   # drag-to-install affordance

rm -f "$OUT"
hdiutil create \
  -volname "BWOC Control Center" \
  -srcfolder "$STAGE" \
  -fs HFS+ -format UDZO -ov \
  "$OUT" >/dev/null

rm -rf "$STAGE"
echo "→ $OUT"
