# 🎛️ bwoc-control-center — Design (Discussion Draft)

> สถานะ: **DISCUSSION / ARCHITECT** · เปิด 2026-06-10 · ยังไม่ scaffold โค้ด
> เป้าหมาย: แอป **mobile (iOS/Android) + desktop (macOS/Win/Linux)** สำหรับ **จัดการ + ตั้งค่า** bwoc fleet/infra จากที่เดียว — ทำงานได้แม้ออกนอกบ้านผ่าน tailnet

> ### ✅ Decisions (เคาะ 2026-06-10)
> 1. **Daemon** = `bwocd` **standalone** (Rust/axum, รีลีสอิสระจาก framework — ไม่ลาก tokio/hyper เข้า bwoc-cli)
> 2. **Auth** = **ed25519 signed requests ครบตั้งแต่ MVP** (ลอก bwoc-party pattern, ไม่มี token-only shortcut)
> 3. **Scope ระยะยาว** = **replace bwoc-mcc + bwoc-devices-app** (consolidate desktop surfaces → retire ทั้งสองเมื่อ control-center พร้อม)
> 4. **Frontend** = **React 19** + Vite + shadcn/Tailwind v4 (ลอก components จาก bwoc-party)

---

## 0. TL;DR (ข้อเสนอ)

| หัวข้อ | ข้อเสนอ |
|---|---|
| **Stack** | **Tauri 2** (Rust core + web frontend) — desktop + iOS + Android จาก codebase เดียว |
| **Frontend** | React 19 + Vite + shadcn/Tailwind v4 (ลอก stack/ส่วนประกอบจาก bwoc-party ได้) |
| **Backend ต่อ bwoc** | **daemon ใหม่ `bwocd`** (axum) ห่อ `bwoc … --json` แล้วเปิดผ่าน **tailnet** (HTTPS via `tailscale serve`) — mobile เข้าทางนี้ |
| **Local mode** | desktop ที่อยู่เครื่องเดียวกับ workspace → Tauri Rust core shell `bwoc --json` ตรง (ไม่ผ่าน network) |
| **Auth** | tailnet (device-level) + **ed25519 signed requests** (ลอกแพทเทิร์น bwoc-party ตรง ๆ) + per-action signing สำหรับ destructive ops |
| **MVP เฟส 1** | read-only fleet over tailnet + พิสูจน์ pipeline iOS build → พิสูจน์ "คุม fleet จากมือถือนอกบ้าน" |

**จุดต่างจากของเดิม:** bwoc-mcc / bwoc-devices-app / bwoc-chat / bwoc-llm-pm = **desktop/macOS ล้วน ไม่มีตัวไหน mobile**. control-center คือ **ตัวแรกที่ cross-platform + remote-over-tailnet** และระยะยาวควร *กลืน* mcc + devices-app เข้ามาเป็น surface เดียว

---

## 1. Tech Stack — ทำไม Tauri 2

> ⚠️ แก้ความเข้าใจผิด: **Tauri 2 stable รองรับ iOS + Android เป็น first-class target** (เป็น headline ของ v2) — ไม่ใช่ "ไม่มี mobile". นี่คือเหตุผลหลักที่เลือก

| ตัวเลือก | desktop | mobile | Rust reuse | เข้ากับ ecosystem | สรุป |
|---|---|---|---|---|---|
| **Tauri 2** ✅ | ✅ mac/win/linux | ✅ iOS/Android | ✅ **link `bwoc-core` ตรง** (เหมือน bwoc-mcp) + ลอก devices-app | สูงสุด (Rust-first, signing crate ใช้ซ้ำ) | **เลือก** |
| React Native | ✗ อ่อน (rn-macos/windows คลุน) | ✅ | ✗ | ต่ำ | ตก |
| Flutter | ✅ | ✅ | ✗ (Dart) | ต่ำ — ภาษาแปลกปลอม | ตก |
| Capacitor | ✗ web-wrap | ✅ | ✗ | ต่ำ | ตก |

**เหตุผลเพิ่ม:**
1. **Rust ecosystem reuse** — Tauri core เป็น Rust จึง `import bwoc-core` / `bwoc-signing` ได้ตรง (แพทเทิร์นเดียวกับ bwoc-mcp ที่ link bwoc-core + shell-out hybrid)
2. **มีของพิสูจน์แล้วในบ้าน** — `bwoc-devices-app` เป็น Tauri 2 อยู่แล้ว (shell `bwoc --json`, IPC commands `list_agents`/`fleet_health`/…) ⇒ ลอก Rust side ได้ทันที
3. **iOS build server เพิ่งตั้ง** — Tauri มี `tauri ios build`/CI flow ที่เข้ากับ self-hosted Mac runner ได้พอดี
4. **frontend codebase เดียว** ใช้ได้ทั้ง 5 แพลตฟอร์ม + ลอก shadcn/Tailwind components จาก bwoc-party
5. binary เล็ก, ใช้ system webview (ไม่ลาก Chromium แบบ Electron)

**Trade-off ที่ต้องรับ:** Tauri mobile ยังใหม่กว่า desktop เล็กน้อย (plugin ecosystem, secure-storage บางตัวต้องเขียนเอง); iOS ต้อง provisioning/signing จริง

---

## 2. Architecture

### ปัญหาแกนกลาง
แพทเทิร์นเดิมทั้ง ecosystem คือ **"shell `bwoc … --json`"** (mcc, devices-app ทำแบบนี้). แต่ **มือถือ shell CLI ไม่ได้** — ไม่มี binary, ไม่มี workspace filesystem, และเครื่องอยู่คนละที่. ⇒ ต้องมี **HTTP boundary** คั่น

ปัจจุบัน bwoc **ไม่มี control HTTP API** (มีแต่ `bwoc a2a serve` = per-agent JSON-RPC สำหรับ agent↔agent เท่านั้น) ⇒ **ต้องสร้าง daemon ใหม่**

### ภาพรวม

```
                                          ┌───────────────── HOST (Mac=anna / bemind=busaba) ──────────────┐
  📱 mobile (iOS/Android)                  │                                                                 │
  ┌───────────────────┐   tailnet HTTPS    │   ┌──────────────┐  shell  ┌─────────────────────────────┐    │
  │ Tauri 2 app        │ ──(tailscale serve)──▶│  bwocd       │ ──────▶ │ bwoc CLI  (--json)          │    │
  │  React UI          │   ed25519-signed   │   │  (axum)      │         │  list/status/inbox/send/... │    │
  │  ───────────────   │                    │   │  127.0.0.1   │  read   ├─────────────────────────────┤    │
  │  Transport trait:  │                    │   │  :10130      │ ──────▶ │ workspace files             │    │
  │   • RemoteHttp ◀───┼────────────────────┘   │              │         │  .bwoc/*, manifests, keys   │    │
  │   • LocalShell  ───┼──┐ (desktop เครื่องเดียวกับ ws)         └──────────────┘         └─────────────────────────────┘    │
  └───────────────────┘  │                  │                                                                 │
  🖥️ desktop (mac/win/linux)│ shell bwoc ตรง  └─────────────────────────────────────────────────────────────┘
        (Tauri Rust core) ◀┘                       ┌── host #2 (bemind) รัน bwocd ของตัวเอง ── คุมหลาย host จากแอปเดียว
```

### หัวใจ: `Transport` trait (ใน Tauri Rust core)
Frontend เรียก IPC command เดียว เช่น `fleet_snapshot()` — Rust core เลือก backend ให้:
- **LocalShell** — desktop + อยู่เครื่องเดียวกับ workspace → `Command::new("bwoc").arg("--json")` (ลอก devices-app)
- **RemoteHttp** — mobile **เสมอ**, หรือ desktop ที่คุม host อื่น → ยิง `bwocd` ผ่าน tailnet พร้อม signed headers

⇒ UI เหมือนกันทุกแพลตฟอร์ม, ต่างแค่ transport. รองรับ **multi-host** (สลับดู anna ↔ busaba) ด้วย host list

### `bwocd` — control daemon (โปรเจกต์ย่อยใหม่, Rust/axum)
- **ห่อ CLI ไม่ reinvent** — ทุก endpoint = shell `bwoc <cmd> --json` (+ อ่าน workspace files ที่ CLI ไม่ครอบคลุม). Logic ความถูกต้องอยู่ที่ CLI ตัวเดียว
- **Infra policy 2026-06-10:** bind `127.0.0.1:10130`, เปิดผ่าน `tailscale serve` HTTPS, **ไม่เปิดพอร์ต public** (เข้าทาง tailnet hostname `bemindlabs.tail32ca95.ts.net` / Mac hostname)
- รันเป็น service ต่อ host (launchd บน Mac, systemd บน bemind)
- *ทางเลือก future:* ผนวกเป็น subcommand `bwoc serve` ใน framework แทน standalone — ดู Q ข้อ 5

---

## 3. Auth & Security (ลอก bwoc-party)

bwoc-party มี pattern พร้อมใช้ — **ใช้ซ้ำได้เกือบ verbatim**:

**Signed request** (header เหมือน party):
```
X-BWOC-Agent       <controller-id>
X-BWOC-Nonce       <16-byte hex>          # replay defense
X-BWOC-Timestamp   <ISO8601>              # ±5 min skew
X-BWOC-Body-SHA256 <hex>
X-BWOC-Signature   ed25519( JCS{sender,method,path,nonce,ts,bodyHash} )
```
ฝั่ง server (`bwocd`) ยืนยัน: ลายเซ็นตรง pubkey + body hash + timestamp window + nonce ไม่ซ้ำ — โค้ด `src/lib/agent-auth.ts` + `src/lib/party-sign/` ลอกมาแปลงเป็น Rust (`bwoc-signing` crate มีอยู่แล้ว)

**Controller identity (ใครเซ็น?):**
- control-center แต่ละ install = **controller keypair ของตัวเอง** (ed25519) สร้างตอน first-run, เก็บใน **secure storage** (iOS Keychain / Android Keystore / macOS Keychain ผ่าน Tauri secure-store plugin)
- ลงทะเบียน pubkey กับ `bwocd` allowlist (ครั้งแรกผ่าน provisioning token / QR จาก desktop ที่ trust แล้ว)
- **ไม่ reuse `.bwoc/agent.key` ของ agent** — controller เป็น principal แยก (มนุษย์คุม fleet ≠ agent)

**Layered security:**
1. **tailnet** = ปราการแรก (เฉพาะ device ในเครือข่ายเราถึง hostname ได้ — public ปิด)
2. **ed25519 signing** = ปราการสอง (พิสูจน์ controller identity + กัน replay)
3. **per-action authorization** — destructive ops (`retire`, `stop`, config write) ต้องเซ็นต่อ action + (option) ยืนยัน biometric บนมือถือ
4. **read vs write scope** ต่อ controller key (อ่านอย่างเดียว vs คุมได้)

**Google login (มนุษย์):** *ไม่จำเป็นใน MVP* (single-operator). เก็บไว้เฟส multi-user / เชื่อม bwoc-party identity ทีหลัง

---

## 4. API Surface (`bwocd` — ข้อเสนอ v1)

REST mirror ของ `bwoc --json` + SSE สำหรับ live. ทุก route ต้อง signed (ยกเว้น `/health`).

| Method | Path | ห่อ CLI / ไฟล์ | scope |
|---|---|---|---|
| `GET` | `/health` | liveness | public |
| `GET` | `/fleet` | `bwoc list --json` + `fleet health --json` | read |
| `GET` | `/agents/:id` | `bwoc status --json` + อ่าน `config.manifest.json` | read |
| `GET` | `/agents/:id/inbox` | `bwoc inbox :id --json` | read |
| `GET` | `/agents/:id/log?follow=1` | `bwoc log` → **SSE** | read |
| `GET` | `/teams` / `/teams/:id/tasks` | `bwoc team list` / `task list --json` | read |
| `POST` | `/agents/:id/send` | `bwoc send` | write |
| `POST` | `/agents/:id/spawn` `…/stop` `…/start` | lifecycle | write |
| `POST` | `/agents/:id/retire` | `bwoc retire` | **write+confirm** |
| `POST` | `/tasks` / `/tasks/:id/{claim,complete,approve}` | task ops | write |
| `GET`/`PUT` | `/agents/:id/config` | manifest / .env / routes.toml | **write+confirm** |
| `GET` | `/hosts` | self-describe (hostname, workspace, bwoc version) | read |

**หมายเหตุ:** interactive flows (`spawn`/`chat` ที่ต้อง TTY) บน mobile = **stream over SSE/WS** (ไม่ใช่เปิด Terminal.app แบบ desktop). อาจ reuse `bwoc-harness --chat` chat_proto (แบบ bwoc-chat) ในเฟสหลัง

---

## 5. Data ที่แอปต้องเข้าถึง (ผ่าน CLI/daemon — ไม่อ่านไฟล์ตรงจาก mobile)

| ข้อมูล | แหล่ง | ใช้ทำอะไร |
|---|---|---|
| agent registry | `.bwoc/agents.toml` (`bwoc list --json`) | fleet list |
| agent health/identity | `bwoc status/fleet health --json` | สถานะ + Aparihāniya-7 signals |
| manifest | `config.manifest.json` | config view/edit (model, trust, cmds) |
| inbox | `.bwoc/inbox.jsonl` (`bwoc inbox --json`) | อ่าน/ส่งข้อความ |
| teams/tasks | `.bwoc/teams/*` (`bwoc team/task --json`) | Saṅgha board |
| routes/peers/gateway | `routes.toml` ฯลฯ | interconnect view (เฟสหลัง) |
| controller key | secure storage บน device | เซ็น request |

---

## 6. MVP — เฟส 1 (พิสูจน์ "คุม fleet จากมือถือนอกบ้าน")

> หลักการ: เฟส 1 ต้องพิสูจน์ **จุดต่าง** (cross-platform + remote-over-tailnet) ไม่ใช่ทำ read-only desktop ซ้ำ devices-app

**Scope:**
1. **`bwocd` ขั้นต่ำ** — axum, `tailscale serve` HTTPS, **ed25519 signed-request verify (เต็มรูปแบบ — ลอก party)**, endpoints: `/health` `/fleet` `/agents/:id` `/agents/:id/inbox` `/hosts` (read-only)
2. **Tauri 2 app (React 19)** — `Transport` trait (LocalShell + RemoteHttp), controller keypair + secure storage + signing client, host list
3. **หน้าจอ:** Fleet (list + health badge) → Agent detail (status + manifest readonly) → Inbox (read)
4. **Ship จริง 2 แพลตฟอร์ม:** macOS `.dmg` + **iOS** (ผ่าน build server ที่เพิ่งตั้ง) — พิสูจน์ pipeline ครบ
5. **Demo สำเร็จ =** เปิดแอปบน iPhone นอกบ้าน (tailnet, signed) เห็น fleet ของ anna + busaba สด

**ตัดออกจากเฟส 1 (ไปเฟส 2+):** mutations (spawn/stop/retire/send), config editing, per-action confirm + biometric UI, SSE log streaming, gateway/party monitor, Ollama model mgmt, MQTT/devices
> หมายเหตุ: ed25519 transport signing อยู่ในเฟส 1 แล้ว; ที่เลื่อนไปเฟส 2 คือ *per-action confirm/biometric UX* เท่านั้น

**เฟส 2:** actions (spawn/stop/retire/send) + per-action confirm + biometric + SSE logs + chat (chat_proto)
**เฟส 3:** config edit, gateway/party/inbox monitor, Ollama models, devices/MQTT → กลืน mcc + devices-app

---

## 7. ❓ คำถามที่ยังเปิดอยู่

> เคาะแล้ว: daemon=standalone · auth=ed25519-full · scope=replace mcc+devices-app · frontend=React 19 (ดู §0 Decisions)

1. **Multi-host จำเป็นใน MVP ไหม?** — เฟส 1 ต้องเห็นทั้ง anna + busaba พร้อมกัน หรือ host เดียวพอก่อน (อีก host เฟสหลัง)? *(กระทบ host-list UI + `/hosts` ตั้งแต่แรก)*
2. **bwocd รันที่ไหนบ้าง?** — เฉพาะ bemind server, หรือบน Mac (anna) ด้วย? คุม Mac workspace จากมือถือต้องมี bwocd บน Mac + `tailscale serve` บน Mac
3. **iOS delivery:** distribute ยังไง — TestFlight / ad-hoc / **self-hosted OTA** (bemindlabs)? มี Apple Developer account + signing cert พร้อมหรือยัง? *(gate ของ "ship iOS จริง" ในเฟส 1)*
4. **Controller registration UX:** ลงทะเบียน controller pubkey กับ bwocd ครั้งแรกยังไง — provisioning token ใน `bwocd.toml`, QR จาก desktop ที่ trust แล้ว, หรือ `bwoc` CLI command ใหม่ (`bwoc controller add <pubkey>`)?
5. **Port + repo:** ใช้ `:10130` ตามที่เสนอไหม (party=10120)? และ repo `bemindlabs/bwoc-control-center` แยกใหม่ใช่ไหม?

---

*— ร่างโดยแอนนา 🎀 · รอ discuss ต่อก่อน scaffold*
