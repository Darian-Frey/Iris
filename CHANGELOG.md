# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com). IDs refer to FEATURES (F-), DECISIONS (D-), ATTACK_VECTORS (AV-), BUGS (BUG-) and IMPROVEMENTS (IMP-).

## [Unreleased]
### Added
- Project documentation set: README, FEATURES (F-001 to F-029), ROADMAP (Phases 0–6), ARCHITECTURE, DECISIONS (D-001 to D-015), PROTOCOL, ATTACK_VECTORS (AV-001 to AV-016), BUILD (unverified draft), BUGS, IMPROVEMENTS, CLAUDE, LICENSE (GPL-3.0-or-later, D-012).
- `tools/phantom_probe.py`: read-only probe of the vendor HID channel.
- `data/phantom_iso_uk_leds.toml`: LED index map for the 88-key ISO-UK board (LEDs 64 and 105 unverified, AV-006).
- `data/60-iris-keyboard.rules`: host udev rule for `320f:5064`.
- Cargo workspace (edition 2024, GPL-3.0-or-later) with an empty `iris-proto` crate that forbids `unsafe` and denies `unwrap`/`expect` outside tests (D-005, F-001 groundwork).
- `iris-proto`: closed `Command` enum holding exactly the allow-list, typed request constructors (no raw command or address API), reply parser with report-ID filtering and echo matching, checksum, configuration and colour address helpers, configuration-block and capability-block parsers, LED map loader; tests for the known-good packets, the allow-list and interleaved reports (F-001 to F-004, D-005, AV-001, AV-008, AV-016).
- Phase 0 reconnaissance completed: hardware identified, EVision V1 protocol confirmed readable on hardware, write path validated through OpenRGB.

### Fixed
- BUG-001: probe no longer sends command `0x0A`, which is a keymap-header write on V1 firmware.
