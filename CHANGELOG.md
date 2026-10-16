# Changelog

## [v2.1.7] - 2026-10-16

chore(release): bump version and publish artifacts


## [v2.1.6] - 2026-10-16

chore(release): bump version and publish artifacts


## [v2.1.5] - 2026-10-16

chore(release): bump version and publish artifacts


## [v2.1.4] - 2026-10-16

chore(release): bump version and publish artifacts


## [v2.1.3] - 2026-10-16

chore(release): bump version and publish artifacts


## [v2.1.2] - 2026-10-16

chore(release): bump version and publish artifacts


## [v2.1.1] - 2026-10-16

chore(release): bump version and publish artifacts


All notable changes to MRT Station are documented in this file.

## [v2.1.0] - 2026-10-16

feat: auto-enable modem ports, Octopus in-row progress, and UI refinements

### What's Changed
- Auto-Enable Modem Port: Implemented automated Samsung dialer (*#0808# -> MTP+ADB) to expose /dev/ttyACM* modem ports for all connected devices.
- Hardware & Local Token Engine: Accurate Samsung model detection and verified token status directly via AT+ENGMODES=9,0.
- Octopus In-Row Progress: Replaced separate progress bars with dual-layer fill tint, 3px bottom accent line, and monospace metric label.
- UI Design System: Extra Large accordion titles, solid light mode header, and compact mobile selection count.
- Standalone 4 EM Cryptographic Modes: Full offline install and token removal via modem AT.
