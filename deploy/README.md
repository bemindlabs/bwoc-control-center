# Deploying bwocd

One daemon per host (DESIGN.md D6). bwocd binds **loopback only** and is
exposed to the tailnet via `tailscale serve` — never a public port.

## 1. Build + install the binary

```bash
cargo build --release --manifest-path bwocd/Cargo.toml
mkdir -p ~/.local/bin && cp bwocd/target/release/bwocd ~/.local/bin/
```

## 2. Register a controller

Each control-center install signs with its own keypair. Get its id + public
key from the app footer (click the pubkey to copy), then append on the host:

```toml
# <workspace>/.bwoc/bwocd/controllers.toml
[[controller]]
id = "ctrl-xxxxxx"
public_key = "<64-char hex>"
scope = "read"
```

bwocd reads the file at startup — restart after editing. (QR + SSO enrollment
replaces this manual step in the next phase-1 slice.)

## 3. Run as a service

**macOS (anna):**

```bash
sed -e "s|__HOME__|$HOME|g" -e "s|__WORKSPACE__|$HOME/workspaces/bwoc|g" \
  deploy/tech.bemind.bwocd.plist > ~/Library/LaunchAgents/tech.bemind.bwocd.plist
launchctl load ~/Library/LaunchAgents/tech.bemind.bwocd.plist
tail -f ~/Library/Logs/bwocd.log
```

**Linux (busaba):**

```bash
mkdir -p ~/.config/systemd/user
cp deploy/bwocd.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now bwocd
journalctl --user -u bwocd -f
```

## 4. Expose over the tailnet

```bash
tailscale serve --bg --https=10130 http://127.0.0.1:10130
tailscale serve status   # verify
```

The host is then reachable at `https://<host>.<tailnet>.ts.net:10130` from
tailnet devices only. Add it to the app:

```toml
# ~/.config/bwoc-control-center/hosts.toml
[[host]]
name = "anna"
url = "https://<host>.<tailnet>.ts.net:10130"

[[host]]
name = "busaba"
url = "https://<host>.<tailnet>.ts.net:10130"
```

## 5. Verify

```bash
curl -s https://<host>.<tailnet>.ts.net:10130/health   # {"ok":true}
# unsigned /fleet must be 401; the app (signed) must render the fleet
```
