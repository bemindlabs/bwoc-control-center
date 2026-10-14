#!/usr/bin/env bash
# Promote pending controller enrollments into the trusted registry.
#
#   scripts/approve-controllers.sh [--workspace DIR] [--list]
#
# --list  show pending entries and exit (no changes).
# Default: move every pending entry into controllers.toml (read scope), clear
# pending, and ask bwocd to reload. Run on each host that should trust them.
set -euo pipefail

WS="$HOME/workspaces/bwoc"
LIST=0
while [ $# -gt 0 ]; do
  case "$1" in
    --workspace) WS="$2"; shift 2 ;;
    --list) LIST=1; shift ;;
    *) echo "usage: $0 [--workspace DIR] [--list]" >&2; exit 2 ;;
  esac
done

DIR="$WS/.bwoc/bwocd"
PENDING="$DIR/pending.toml"
CONTROLLERS="$DIR/controllers.toml"
[ -f "$PENDING" ] || { echo "no pending enrollments ($PENDING)"; exit 0; }

python3 - "$PENDING" "$CONTROLLERS" "$LIST" <<'PY'
import sys, pathlib
try:
    import tomllib  # Python 3.11+
except ModuleNotFoundError:
    try:
        import tomli as tomllib  # same API, for Python < 3.11
    except ModuleNotFoundError:
        sys.exit("error: needs Python 3.11+ (tomllib) or the 'tomli' package")
pending_path, controllers_path, list_only = sys.argv[1], sys.argv[2], sys.argv[3] == "1"
pending = tomllib.load(open(pending_path, "rb")).get("pending", [])
if not pending:
    print("no pending enrollments"); sys.exit(0)

cp = pathlib.Path(controllers_path)
existing = tomllib.load(open(cp, "rb")).get("controller", []) if cp.is_file() else []
known = {c["id"] for c in existing}

print(f"{len(pending)} pending:")
for p in pending:
    print(f"  {p['id']}  {p['public_key'][:16]}…  (since {p.get('first_seen','?')})"
          + ("  [already trusted]" if p['id'] in known else ""))
if list_only:
    sys.exit(0)

added = 0
lines = []
for p in pending:
    if p["id"] in known:
        continue
    lines.append(f'[[controller]]\nid = "{p["id"]}"\npublic_key = "{p["public_key"]}"\nscope = "read"\n')
    added += 1
if added:
    with open(cp, "a") as f:
        if cp.stat().st_size and not open(cp).read().endswith("\n"):
            f.write("\n")
        f.write("\n" + "\n".join(lines))
# clear pending (approved or already-known)
open(pending_path, "w").write("")
print(f"approved {added} new controller(s) → {controllers_path}")
PY

# bwocd reads controllers.toml at startup — nudge it to reload.
if launchctl list 2>/dev/null | grep -q tech.bemind.bwocd; then
  launchctl kickstart -k "gui/$(id -u)/tech.bemind.bwocd" && echo "restarted bwocd (launchd)"
elif systemctl --user is-active bwocd >/dev/null 2>&1; then
  systemctl --user restart bwocd && echo "restarted bwocd (systemd)"
else
  echo "NOTE: restart bwocd manually so it reloads controllers.toml"
fi
