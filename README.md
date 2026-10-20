# MRT Station

Modernized Samsung Engineering Mode (EM) Token Monitoring & Expiration Sentinel built with **Rust / Tauri 2** and **Docker**.

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
                      | Fleet Heartbeat & Token Telemetry     |
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
   - Runs locally on technician workstations with background system tray support.
   - Scans connected Samsung devices via `rusb`, udev `/dev/serial/by-id` symlinks, and ADB.
   - **Auto-Enable Modem Port**: Automatically detects devices in ADB-only mode and triggers Samsung `USBSettings` (`*#0808#` via dynamic UI hierarchy parsing) to expose composite `MTP+ADB` with `/dev/ttyACM*` modem interface.
   - **Hardware TrustZone Token Engine**: Reads active token state, expiration date, and authorized mode bitmask directly from TrustZone modem hardware (`AT+ENGMODES=0,0,0,0` / `9,0`).
   - **Offline Token Erase**: Allows revoking/erasing active token directly over modem port (`AT+ENGMODES=0,0,3,0`).
   - Exposes an embedded local Axum HTTP/WebSocket API on port `7890`.

2. **Central Web Dashboard**
   - Packaged as a lightweight Docker container on port `3010`.
   - Aggregates multi-node fleet inventory in real-time.
   - **Fleet Token Expiration Sentinel**: Live countdown badges showing remaining validity (Aktif, Segera Habis / <= 14 hari, Expired, atau No Token).
   - **EM Mode Badges**: Direct visualization of active authorized capabilities (`ENG` for kernel, `SUW` for setup wizard skip, `GA_SS` for screen capture authorization).
   - **Global Live Terminal**: Real-time AT command streaming drawer for tracking fleet communication and diagnostic responses.

---

## Supported Samsung EM Modes

| Mode Index | Mode Constant | Description | UI Badge |
|---|---|---|---|
| `0` | `MODE_ENG_KERNEL` | Eng Kernel Execution Mode (Root & SELinux Permissive) | `ENG` |
| `1` | `MODE_SKIP_SUW` | Skip Setup Wizard (Bypass initial OOBE setup) | `SUW` |
| `12` | `MODE_ALLOW_GET_GA_SCREENSHORT` | Authorize screen capture / GA screenshot capture | `GA_SS` |
| `49` | `MODE_ALLOW_GET_GA_SCREENSHORT` | Alternate authorization for GA screenshot capture | `GA_SS` |

---

## Design System & Accessibility

- **Font**: SamsungOne corporate typography via CDN (`https://fonts.cdnfonts.com/css/samsungone`). Monospace data in JetBrains Mono.
- **Themes**: High-contrast Dark (default) and Light modes with solid surface headers and WCAG AA compliance.
- **Icons**: Clean inline SVG symbols with zero emojis.
- **Responsiveness**: Multi-breakpoint responsive layout with compact mobile badges.
- **Ponytail Philosophy**: Radical simplicity, zero speculative settings menus, flush single-level fleet table without nested accordions.

---

## Quick Start

### 1. Run Central Web Dashboard (Docker)

```bash
# Build and start via docker-compose on port 3010
docker compose up -d --build
```

Access the dashboard at `http://localhost:3010` or through Cloudflare tunnel at `https://mrt.endrisusanto.my.id`.

### 2. Run Desktop Node Agent (Tauri)

```bash
# Install frontend dependencies
npm install

# Run in development mode (spawns desktop app and node API on port 7890)
npm run dev

# Or build and install native Debian package
./build-deb.sh --install
```

---

## API Reference (Node Agent: Port 7890)

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/health` | Health check and agent version |
| `GET` | `/info` | Node metadata, IP, OS, and uptime |
| `GET` | `/devices` | List connected Samsung devices with verified tokens and modem ports |
| `POST` | `/devices/refresh` | Force rescanning USB bus and re-checking modem AT ports |
| `POST` | `/devices/:serial/query` | Read TrustZone token status and expiration via `AT+ENGMODES=0,0,0,0` |
| `POST` | `/devices/:serial/remove-token` | Erase active token offline via modem command `AT+ENGMODES=0,0,3,0` |
| `WS` | `/ws` | Real-time WebSocket event stream |

---

## Release Pipeline

MRT Station includes an automated release pipeline in [`release.sh`](./release.sh):

```bash
# Release a patch version
./release.sh patch "fix: update mode definitions and remove settings menu"

# Release a minor version
./release.sh minor "feat: pure token expiration sentinel"
```

The script increments versions across `Cargo.toml`, `package.json`, and `tauri.conf.json`, compiles and tests the `.deb` package, tags the commit with release notes, updates `CHANGELOG.md`, and triggers multiplatform GitHub Actions builds.
