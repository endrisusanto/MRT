# MRT Station Modernized

Modernized Samsung EM Station distributed diagnostics system built with **Rust / Tauri 2** and **Docker**.

## Architecture Overview

```
                      +------------------------------------------+
                      |   Cloudflare Tunnel                      |
                      |   https://mrt.endrisusanto.my.id         |
                      +--------------------+---------------------+
                                           |
                                           v
                              +------------------------+
                              | Central Web Dashboard  |
                              | Docker Container       |
                              | Port: 3010             |
                              +-----------+------------+
                                          |
                      +-------------------+-------------------+
                      | Heartbeat & Commands                  |
                      v                                       v
         +--------------------------+           +--------------------------+
         | Technician PC 1 (Node)   |           | Technician PC 2 (Node)   |
         | Tauri Desktop App (Tray) |           | Tauri Desktop App (Tray) |
         | Axum API: Port 7890      |           | Axum API: Port 7890      |
         +------------+-------------+           +------------+-------------+
                      |                                       |
             +--------+--------+                     +--------+--------+
             | Samsung Devices |                     | Samsung Devices |
             +-----------------+                     +-----------------+
```

### Key Components & Capabilities

1. **Tauri Desktop App (Node Agent)**
   - Runs locally on technician workstations with tray minimization (`CloseRequested` intercepted).
   - Scans connected Samsung devices via `rusb`, udev `/dev/serial/by-id` symlinks, and ADB.
   - **Auto-Enable Modem Port**: Automatically detects devices in ADB-only mode and triggers Samsung `USBSettings` (`*#0808#` via dynamic UI hierarchy parsing) to expose composite `MTP+ADB` with `/dev/ttyACM*` modem interface.
   - **Hardware & Local Token Engine**: Validates active cryptographic tokens directly against device modem hardware (`AT+ENGMODES=9,0`) and local secure store (`~/.mrt-station/tokens.json`).
   - Exposes an embedded local Axum HTTP/WebSocket API on port `7890`.

2. **Central Web Dashboard**
   - Packaged as a lightweight Docker container on port `3010`.
   - Aggregates multi-node fleet inventory in real-time.
   - **Octopus-styled Progress Tracking**: In-row background fill tint (`rgba(56, 189, 248, 0.18)`), 3px bottom accent line, and right-aligned metric label (`${step}: ${pct}%`) directly on device rows.
   - **Modern Technical UI**: Extra Large commanding accordion headers, token status badges with icon state, and mobile-optimized compact counters.
   - **Global Live Terminal**: Real-time AT command streaming drawer for tracking fleet operations.

---

## Design System & Accessibility

- **Font**: SamsungOne corporate typography via CDN (`https://fonts.cdnfonts.com/css/samsungone`). Monospace data in JetBrains Mono.
- **Themes**: High-contrast Dark (default) and Light modes with solid surface headers and WCAG AA compliance.
- **Icons**: Clean inline SVG symbols with zero emojis.
- **Progress Fill**: Octopus in-row dual-layer progress bar and fill.
- **Responsiveness**: Multi-breakpoint responsive layout with compact mobile badges (hiding suffix labels on mobile).

---

## Quick Start

### 1. Run Central Web Dashboard (Docker)

```bash
# Build and start via docker-compose on port 3010
docker compose up -d --build
```

Access the dashboard at `http://localhost:3010` or through your Cloudflare tunnel at `https://mrt.endrisusanto.my.id`.

### 2. Run Desktop Node Agent (Tauri)

```bash
# Install frontend dependencies
npm install

# Run in development mode (spawns desktop app and node API on port 7890)
npm run dev

# Or build native Debian package
./build-deb.sh --install
```

---

## API Reference (Node Agent - Port 7890)

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/health` | Health check and agent version |
| `GET` | `/info` | Node metadata, IP, OS, and uptime |
| `GET` | `/devices` | List connected Samsung devices with verified tokens and modem ports |
| `POST` | `/devices/:serial/install` | Install 4 standalone EM modes (`0`, `1`, `12`, `49`) |
| `POST` | `/devices/:serial/remove-token` | Remove active token offline via modem command `AT+ENGMODES=0,0,3,0` |
| `POST` | `/devices/:serial/uninstall` | Uninstall Samsung agent from target device |
| `POST` | `/devices/:serial/recover` | Run diagnostics recovery on target device |
| `WS` | `/ws` | Real-time WebSocket event stream |

---

## Supported Standalone EM Modes

| Index | Mode Name | Description |
|---|---|---|
| `0` | `MODE_ENG_KERNEL` | Eng Kernel Execution Mode (Root & Selinux Permissive) |
| `1` | `MODE_INIT_EM` | Master EM Subsystem Initializer & Security State Override |
| `12` | `MODE_ACCESS_SOD` | Secure Operations & Debugger Access Interface |
| `49` | `MODE_EXTRACT_RAM_DUMP` | Physical RAM Dump Extraction directly via modem interface |

---

## Release Pipeline

MRT Station includes an automated release pipeline in [`release.sh`](./release.sh):

```bash
# Release a patch version
./release.sh patch "Fix device bridge auto-reconnect"

# Release a minor version with custom changelog message
./release.sh minor "Add auto-enable modem port and Octopus in-row progress"
```

The script increments versions across `Cargo.toml`, `package.json`, and `tauri.conf.json`, compiles and tests the `.deb` package, tags the commit with release notes, updates `CHANGELOG.md`, and triggers multiplatform GitHub Actions builds.
