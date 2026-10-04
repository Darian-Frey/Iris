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
- `crates/iris-proto/fixtures/probe-2026-10-04.txt`: raw read-only probe capture from the reference board; `iris-proto` parser tests now run on it instead of reconstructed payloads (IMP-004). PROTOCOL.md records the full 44-byte capability block, the changed profile 1 settings, block byte `0x13` (OPEN) and the now non-empty custom map. The configuration stride `0x2A` is promoted from SRC to HW on the author's decision; the profile 1 cross-check is closed.
- `iris-device` crate: `Transport` trait whose only send method takes an `iris_proto::Request`, and an in-memory simulated keyboard (`sim` feature) that speaks EVision V1, starts from the 2026-10-04 capture, reproduces its replies byte for byte, and records protocol misuse (writes outside a transaction, nested begin, stray end, bad checksum) for tests; supports injected key-press reports, dropped replies and unplugging (D-005, AV-005, AV-008, AV-014, F-012).
- `iris-device`: sysfs discovery that selects the vendor interface by parsing the report descriptor; `Hidraw` transport with an advisory exclusive lock and disconnection mapping; `Device` with the identity check (V1 magic required; writes only for `320f:5064` firmware 1.02), reply matching that skips key presses and stale replies, and a `Transaction` guard that sends end on every exit path; `permission_help()` with the udev rule; `discover` and read-only `read_state` examples (F-001, F-025, AV-005, AV-007, AV-008, AV-009, AV-010, AV-014). The read path was confirmed on the reference keyboard by the author: identity verified, all reads matched, zero writes.
- CLAUDE.md and ARCHITECTURE.md: read-only development tools and the Phase 1 direct-mode `irisctl` may open the hidraw node through `iris-device` (author's decision).
- `iris_proto::capture`: reads replies back out of probe output; shared by the parser tests and the simulator.
- Phase 0 reconnaissance completed: hardware identified, EVision V1 protocol confirmed readable on hardware, write path validated through OpenRGB.

### Fixed
- BUG-001: probe no longer sends command `0x0A`, which is a keymap-header write on V1 firmware.
