# Roadmap

Phases are append-only. Mark complete with an ISO date; do not delete.

## Phase 0 — Reconnaissance
**Goal:** Establish what the keyboard is, what protocol it speaks, and whether that protocol works on this exact board.
**Status:** Complete (2026-09-20), with three hardware checks carried into Phase 1
**Features delivered:** none (knowledge only)
**Deliverables:**
- [x] Prior-art survey: dokutan/rgb_keyboard and OpenRGB EVision controllers read at source level
- [x] Hardware identified as `320f:5064`, firmware 1.02, interface layout recorded (PROTOCOL.md §1)
- [x] Read-only probe (`tools/phantom_probe.py`); replies decoded (PROTOCOL.md §4, §5)
- [x] V1 write path validated on hardware through OpenRGB; single-key colour confirmed
- [x] LED index map derived (`data/phantom_iso_uk_leds.toml`); LED 59 = J confirmed
- [x] Host udev rule written and working
- [ ] Carried: confirm LEDs 64 and 105 (PROTOCOL.md §9 item 1)
- [ ] Carried: confirm all keys and Fn combinations behave normally after BUG-001 (§9 item 6)
- [x] Carried: cross-check decoded profile 1 settings against the keyboard's visible behaviour (2026-10-04: profile 1 had been changed with the Fn keys; the new values, mode `0x04`, brightness 4, speed 0, show as a bright colour cycle)
**Acceptance:** A documented protocol with every fact tagged HW / SRC / OPEN, and a known-safe command set.

## Phase 1 — Device core
**Goal:** Talk to the keyboard safely from Rust, from inside a Flatpak, with a minimal CLI.
**Status:** In progress (`iris-proto` done 2026-10-03; simulated keyboard done 2026-10-04)
**Features delivered:** F-001, F-002, F-003, F-004, F-012 (device-side: diffing and counter), F-025; F-023 and F-024 in skeleton form
**Deliverables:**
- [x] Cargo workspace; `iris-proto` with packet builder/parser, closed command enum, address helpers, capability parser, LED map loader
- [x] Unit tests including the allow-list test (AV-001) and interleaved-report test (AV-008) (2026-10-03; AV-008 at packet level, device-level test follows with `iris-device`)
- [x] In-memory simulated keyboard implementing V1 (ARCHITECTURE.md §Testing seam) (2026-10-04: `iris_device::sim`, behind the `sim` feature)
- [ ] `iris-device`: sysfs discovery, identity check (AV-007), exchange with echo matching, transaction guard (AV-005), hotplug
- [ ] Temporary direct-mode `irisctl` (talks to the device itself, no daemon yet): `info`, `get`, `set-mode`, `set-key`, `set-map`, `walk` (lights one LED at a time for AV-006)
- [ ] Skeleton Flatpak manifest that builds the CLI offline and runs it against the real keyboard; BUILD.md corrected from that experience
- [ ] Hardware experiments PROTOCOL.md §9 items 1, 2, 3 run and recorded; carried Phase 0 checks closed
**Acceptance:** From inside the sandbox, without root: read full device state; set a mode; upload a full 88-key map with every key correct; setting the same map twice sends zero write packets the second time; typing is never interrupted; apply latency and flash-vs-RAM are recorded in PROTOCOL.md.

## Phase 2 — Daemon, profiles and automation
**Goal:** The keyboard changes with the focused application, with no window open.
**Status:** Not started
**Features delivered:** F-007, F-008, F-009, F-012 (complete: rate limit), F-022, F-023 (complete), F-029
**Deliverables:**
- [ ] D-Bus interface definition written and reviewed before implementation
- [ ] `irisd`: shadow state, limiter, profile store with validation, rule engine with priority and fallback
- [ ] X11 `FocusSource` with debounce (AV-013)
- [ ] Idle, lock and suspend handling with re-discovery on resume (AV-014)
- [ ] First-write snapshot and restore (F-029)
- [ ] `irisctl` rewritten as a pure D-Bus client
**Acceptance:** With only `irisd` running, switching between three applications with distinct profiles changes the keyboard correctly within about a second; a burst of 20 focus changes in two seconds produces one write transaction; suspend/resume and unplug/replug both recover without intervention.

## Phase 3 — Graphical editor
**Goal:** A GUI good enough that the CLI is optional.
**Status:** Not started
**Features delivered:** F-013, F-015, F-016, F-014, F-020
**Deliverables:**
- [ ] Key geometry data for the ISO-UK layout; keyboard widget
- [ ] Painting, selection groups, gradient, mirror, palettes
- [ ] Effect panel driven by the advertised mode list
- [ ] Profile manager and rule editor with window picker
- [ ] Tray icon
**Acceptance:** A new profile with a per-key map and a match rule can be created, applied and auto-triggered without touching a file or a terminal.

## Phase 4 — Packaging and first release
**Goal:** Installable on the author's desktop as a finished Flatpak.
**Status:** Not started
**Features delivered:** F-024 (complete), F-026
**Deliverables:**
- [ ] Full manifest: daemon, CLI, GUI, vendored cargo sources, desktop file, icon, metainfo
- [ ] Background-portal autostart; on-demand daemon start from GUI and CLI (AV-012)
- [ ] Permission-failure guidance verified on a machine without the udev rule (AV-010)
- [ ] Version 0.1.0 tagged; CHANGELOG cut
**Acceptance:** On a clean Mint 22.3 install: add the udev rule, install the Flatpak, log out and in, and profiles switch automatically.

## Phase 5 — Refinements
**Goal:** The features other controllers leave out.
**Status:** Not started
**Features delivered:** F-019, F-010, F-011, F-017, F-018, F-021, F-006, F-005 (if its gate has cleared)
**Deliverables:**
- [ ] Colour calibration with a guided white-balance step
- [ ] Night schedule; timed event profiles
- [ ] Legend panel; simple keybind import
- [ ] Opt-in heatmap with counts-only storage (AV-015)
- [ ] `0x04` read-modify-write experiment (PROTOCOL.md §9 item 4), then onboard slot switching
**Acceptance:** Each feature meets its FEATURES.md criteria; AV-015 storage review signed off.

## Phase 6 — Input configuration (gated)
**Goal:** Remapping and macros, if and only if they can be done safely.
**Status:** Not started (blocked by D-015)
**Features delivered:** F-027, F-028
**Deliverables:**
- [ ] Windows VM with USB passthrough; usbmon/Wireshark captures of the Tecware software: read keymap, write keymap, factory reset, macro write
- [ ] Factory reset procedure verified on the reference board
- [ ] PROTOCOL.md extended from captures; allow-list extended by decision entry
- [ ] Remap editor with revert-on-timeout
**Acceptance:** A remap round-trips; an unconfirmed remap reverts by itself; factory state is restorable from within Iris.
