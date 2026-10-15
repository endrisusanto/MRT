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

### Components

1. **Tauri Desktop App (Node Agent)**
   - Runs locally on technician workstations.
   - Minimizes to system tray when the window is closed (`CloseRequested` event intercepted).
   - Scans connected Samsung/Android devices via `rusb` and `serialport`.
   - Exposes an embedded local HTTP/WebSocket API on port `7890`.
   - Sends background heartbeats to the Central Dashboard.
   - Single Sign-On (SSO) has been completely eliminated for seamless offline / Debian / Ubuntu / Windows usage.

2. **Central Web Dashboard**
   - Packaged as a lightweight Docker container on port `3010`.
   - Proxied via Cloudflare Tunnel (`mrt.endrisusanto.my.id`).
   - Discovers and aggregates multiple distributed station nodes.
   - Provides real-time device status and remote operation dispatching via WebSocket.

## Design System & Accessibility

- **Font**: SamsungOne corporate typography via CDN (`https://fonts.cdnfonts.com/css/samsungone`). Monospace data in JetBrains Mono.
- **Type Scale**: Compact technical scale (10px to 18px).
- **Themes**: High-contrast Dark (default) and Light modes with WCAG AA compliance (contrast ratios > 16:1).
- **Icons**: Clean inline SVG symbols with zero emojis anywhere in the codebase.
- **Progress Indicators**: Circular SVG ring progress (battery/device level) and filled underline progress bar (operations).
- **Responsiveness**: Multi-breakpoint layout (<480px, 480-768px, >768px) with bottom navigation on mobile.

## Quick Start

### 1. Run Central Web Dashboard (Docker)

```bash
# Build and start via docker-compose on port 3010
docker compose up -d
```

Access the dashboard at `http://localhost:3010` or through your Cloudflare tunnel at `https://mrt.endrisusanto.my.id`.

### 2. Run Desktop Node Agent (Tauri)

```bash
# Install frontend dependencies
npm install

# Run in development mode (spawns desktop app and node API on port 7890)
npm run dev

# Or build release bundle (.msi / .deb / .AppImage)
npm run build
```

## API Reference (Node Agent - Port 7890)

| Method | Endpoint | Description |
|---|---|---|
| `GET` | `/health` | Health check and agent version |
| `GET` | `/info` | Node metadata, IP, OS, and uptime |
| `GET` | `/devices` | List connected Samsung/Android devices |
| `POST` | `/devices/:serial/install` | Install Samsung agent on target device |
| `POST` | `/devices/:serial/uninstall` | Uninstall Samsung agent from target device |
| `POST` | `/devices/:serial/recover` | Run diagnostics recovery on target device |
| `WS` | `/ws` | Real-time WebSocket event stream |

## GitHub Actions CI/CD

Automated multiplatform builds are configured in `.github/workflows/build-all.yml`:
- Windows: generates `.msi` and `.exe` installers.
- Linux: generates `.deb` and `.AppImage` packages.
- Docker: builds production dashboard image.
