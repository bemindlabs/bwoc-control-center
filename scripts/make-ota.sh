#!/usr/bin/env bash
# Assemble an over-the-air (OTA) install bundle from a signed .ipa:
# a manifest.plist (itms-services) + an install index.html. Output goes to
# out/ota/, ready to serve over HTTPS (iOS requires HTTPS for OTA install).
#
#   BASE_URL=https://host.tailnet.ts.net:10131 scripts/make-ota.sh
#
# The device must be in the .ipa's provisioning profile (4 dev devices here)
# AND able to reach BASE_URL (e.g. on the tailnet).
set -euo pipefail
cd "$(dirname "$0")/.."

IPA="${IPA:-src-tauri/gen/apple/build/arm64/BWOC Control Center.ipa}"
BASE_URL="${BASE_URL:?set BASE_URL to the HTTPS root that will serve out/ota}"
BASE_URL="${BASE_URL%/}"
[ -f "$IPA" ] || { echo "error: no .ipa at $IPA — run scripts/ios-build.sh device" >&2; exit 1; }

OUT="out/ota"
rm -rf "$OUT"; mkdir -p "$OUT"
cp "$IPA" "$OUT/app.ipa"

# read identity from the built app — extract the (binary) Info.plist to a real
# file; piping it through `head`/`printf` mangles binary plists. No hardcoded
# fallbacks: a manifest with the wrong bundle id/version installs nothing, so
# fail loudly instead. The trap cleans the temp dir on every exit path.
PL_TMP=$(mktemp -d)
trap 'rm -rf "$PL_TMP"' EXIT
unzip -q -o "$IPA" "Payload/*/Info.plist" -d "$PL_TMP"
# -print -quit, not `| head -1`: head closing the pipe can SIGPIPE find, which
# pipefail turns into a spurious failure.
PL=$(find "$PL_TMP" -name Info.plist -path "*.app/Info.plist" -print -quit)
[ -n "$PL" ] || { echo "error: no Payload/*.app/Info.plist in $IPA" >&2; exit 1; }
BID=$(plutil -extract CFBundleIdentifier raw -o - "$PL") \
  || { echo "error: cannot read CFBundleIdentifier from $PL" >&2; exit 1; }
VER=$(plutil -extract CFBundleShortVersionString raw -o - "$PL") \
  || { echo "error: cannot read CFBundleShortVersionString from $PL" >&2; exit 1; }
TITLE="BWOC Control Center"

cat > "$OUT/manifest.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>items</key><array><dict>
  <key>assets</key><array><dict>
    <key>kind</key><string>software-package</string>
    <key>url</key><string>${BASE_URL}/app.ipa</string>
  </dict></array>
  <key>metadata</key><dict>
    <key>bundle-identifier</key><string>${BID}</string>
    <key>bundle-version</key><string>${VER}</string>
    <key>kind</key><string>software</string>
    <key>title</key><string>${TITLE}</string>
  </dict>
</dict></array></dict></plist>
PLIST

cat > "$OUT/index.html" <<HTML
<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>${TITLE} — install</title>
<style>
  :root{color-scheme:dark}
  body{margin:0;min-height:100vh;display:grid;place-items:center;
    background:oklch(0.205 0.014 250);color:oklch(0.96 0.005 230);
    font-family:-apple-system,system-ui,sans-serif}
  .card{max-width:22rem;padding:2rem;border:1px solid oklch(1 0 0/12%);
    border-radius:14px;background:oklch(0.245 0.015 250);text-align:center}
  .logo{font-size:2.5rem}
  h1{font-size:1.15rem;margin:.6rem 0 .2rem}
  p{color:oklch(0.71 0.012 240);font-size:.85rem;margin:.2rem 0 1.2rem}
  a.btn{display:block;padding:.7rem 1rem;border-radius:10px;text-decoration:none;
    font-weight:600;background:oklch(0.72 0.10 200);color:oklch(0.20 0.02 240)}
  small{display:block;margin-top:1rem;color:oklch(0.71 0.012 240);font-size:.72rem}
</style></head><body><div class="card">
  <div class="logo">🎛️</div>
  <h1>${TITLE}</h1>
  <p>v${VER} · iOS / iPadOS</p>
  <a class="btn" href="itms-services://?action=download-manifest&amp;url=${BASE_URL}/manifest.plist">Install</a>
  <small>Open in Safari on a registered device. Requires the device on the tailnet.</small>
</div></body></html>
HTML

echo "→ $OUT  (ipa $(du -h "$OUT/app.ipa" | cut -f1), bid=${BID}, v${VER})"
echo "  serve it at ${BASE_URL}/  then open that URL in Safari on a device"
