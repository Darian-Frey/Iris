# CLAUDE.md

## Project
Iris is a Linux controller for the Tecware Phantom RGB keyboard (EVision V1 protocol, USB `320f:5064`): a Rust daemon that owns the device and switches lighting profiles by focused application, a Rust CLI, and a C++/Qt 6 editor, all shipped as one Flatpak. The protocol is reverse-engineered and the hardware cannot be re-imaged, so safety of what is sent to the device outranks every other concern.

## Current state
- Documentation set: complete first draft, 2026-09-20. Production code: `iris-proto` only.
- `tools/phantom_probe.py`: working, read-only, run successfully on the reference keyboard.
- `data/phantom_iso_uk_leds.toml`: derived; 86 of 88 entries trusted, LEDs 64 and 105 unverified.
- `data/60-iris-keyboard.rules`: working on the reference machine.
- `BUILD.md`: a plan, not a record. Nothing in it has been executed except the udev section.
- `crates/iris-proto` (2026-10-03): closed `Command` enum, typed request constructors, reply parser, checksum, address helpers, config-block and capability parsers, LED map loader; 33 unit tests; clippy and fmt clean. Parser tests run on the author's raw probe capture of 2026-10-04 (`crates/iris-proto/fixtures/`, IMP-004).
- `crates/iris-device` (2026-10-04): `Transport` trait (sends only `iris_proto::Request`) and, behind the `sim` feature, `sim::SimulatedKeyboard`, which starts from the 2026-10-04 capture and reproduces its replies byte for byte; 14 tests. Since extended (2026-10-04): sysfs discovery, `Hidraw` transport (rustix `poll`/`flock`, no `unsafe`), `Device` with identity check and reply matching, `Transaction` guard, `permission_help()`; 35 tests. Examples: `discover` (sysfs only; finds `/dev/hidraw1` here) and `read_state` (reads only; run by the author on hardware 2026-10-04: identity `Verified`, all reads matched, zero writes).
- `iris-proto` also exposes `capture`, which reads replies back out of probe output.
- `crates/irisd`, `crates/irisctl`, `gui/`, `flatpak/`: empty placeholder directories; no code yet.
- 2026-10-04 observations (PROTOCOL.md §5, §7): the author changed profile 1 and its colour map with OpenRGB between the probe and `read_state`; OpenRGB left the random byte at `0x01` (meaning OPEN) and colour slots 49 and 65 set, which are not in the LED map. Two consecutive `read_state` runs were identical, so colour reads return stored memory.
- Outstanding hardware checks the author has not yet reported back on: post-BUG-001 key check; LEDs 64/105; OpenRGB apply latency. (Profile 1 cross-check closed 2026-10-04: changed by the author with the Fn keys; reads mode `0x04`, brightness 4, speed 0 and shows as a bright colour cycle.)
- Cross-references between documents were aligned by hand and may be stale (IMP-003).

## Active task
Phase 1 — Device core (ROADMAP.md). Start with `iris-proto`:
1. ~~Cargo workspace with `crates/iris-proto`.~~ Done 2026-10-03.
2. ~~`Command` as a closed enum containing exactly the PROTOCOL.md allow-list; packet builder and parser; checksum; config and colour address helpers; capability-block parser; LED map loader.~~ Done 2026-10-03.
3. ~~Tests: checksum against the known-good packets in PROTOCOL.md (`04 2f 00 03 2c…`, `04 67 00 05 38 2a…`); the allow-list test (AV-001); parsing of the three captured config replies and the capability reply.~~ Done 2026-10-03; fixtures replaced with a real capture 2026-10-04 (IMP-004).
4. ~~The in-memory simulated keyboard.~~ Done 2026-10-04 (`iris_device::sim`).
5. ~~`iris-device` proper: sysfs discovery, identity check (AV-007), exchange with report-ID filtering and echo matching (AV-008), transaction guard (AV-005), hotplug.~~ Done 2026-10-04; read path confirmed on hardware the same day.
6. Next: write minimisation (F-012 device side: shadow state and diffing), then the temporary direct-mode `irisctl` (ROADMAP Phase 1).

Acceptance for the phase is in ROADMAP.md. Features: F-001, F-002, F-003, F-004, F-012 (device side), F-025.

## Invariants
- Only allow-listed commands are ever sent: `0x01 0x02 0x03 0x05 0x06 0x10 0x11`. There must be no function anywhere, including `tools/` and tests that touch hardware, that sends an arbitrary command byte (D-005).
- Never send EVision V2 commands, and never adapt code from OpenRGB's `EVisionV2KeyboardController` (D-004, AV-002).
- Never implement `0x04`, `0x08` or `0x0A` without the gating experiment or captures named in PROTOCOL.md §9 and D-015.
- Never synthesise a block containing bytes of unknown meaning; read, patch, write back (AV-016).
- Only `irisd` opens the hidraw node in shipped code. Exception (author's decision, 2026-10-04): read-only development tools (for example `iris-device`'s `read_state` example) and the Phase 1 temporary direct-mode `irisctl` may open it. They must go through `iris-device` (allow-list, identity check, transaction guard, advisory lock), and anything that writes is still run by the author. Interface 0 is never touched; no libusb (D-003).
- Every begin has an end on all exit paths (AV-005).
- Diff against shadow state before writing; all writes go through the rate limiter (F-012).
- A PROTOCOL.md fact moves from SRC to HW only when the author reports a hardware observation. Do not promote by inference.
- The heatmap stores counts and nothing else (AV-015).

## Build & test
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
# hardware, read-only:
python3 tools/phantom_probe.py /dev/hidraw1
```
See BUILD.md for the (unverified) Flatpak commands. Anything that writes to the real keyboard is run by the author, not by an AI session; hand over the exact command and say what it will send.

## Conventions
- British English throughout, in code comments, documentation and UI strings. ISO 8601 dates.
- Rust: stable toolchain, `rustfmt` defaults, `clippy` clean, no `unsafe` outside `iris-device`, no `unwrap` on anything derived from device or user input.
- C++: C++20, Qt 6.4 API ceiling, Qt naming style in the GUI.
- Keys are addressed by the names in `data/phantom_iso_uk_leds.toml` everywhere above `iris-proto`; LED indices and byte offsets do not leak upward.
- Stable IDs (F-, D-, AV-, BUG-, IMP-) in commit messages and CHANGELOG entries. IDs are append-only.
- Commit messages are multi-paragraph: what changed, why, and which IDs. The Subtle Chaos anomaly is suspended for this repository (author's decision, 2026-10-03): do not add one, and do not ask for it unless the author reinstates it.
- Docs are part of the commit. A code change that invalidates a document without updating it is incomplete.
- Bugs and improvement ideas found along the way are logged in BUGS.md / IMPROVEMENTS.md, not acted on silently (Maintenance Rule 8).
- Refresh the README four-field header whenever Last reviewed is stale.

## Pitfalls
- See ATTACK_VECTORS.md for the canonical list.
- The reply status byte is always `0x00` so far, even for a misused command. It proves nothing.
- The same hidraw node delivers key presses (report ID 1) between your request and its reply. Filter on report ID `0x04` and the command/offset echo.
- This board has no OUT endpoint on interface 1; plain `write()` on hidraw is correct and becomes a control transfer. Do not go looking for endpoint `0x03`.
- dokutan's data was captured on `0c45:652f`. Offsets for colours and parameters have held up; his canned capability block and ISO keymap packets must not be reused.
- OpenRGB and dokutan disagree on brightness and speed ranges. Do not pick one; PROTOCOL.md §9 item 3 settles it on hardware.
- Inside the Flatpak, `_NET_WM_PID` is useless (D-007). Do not add process-based rule matching.
- udev enumeration is unreliable in the sandbox; discover through `/sys/class/hidraw`.
- hidraw numbering changes across replug and resume.

## Out of scope
Do not change without asking:
- The command allow-list, and the HW / SRC / OPEN tags in PROTOCOL.md.
- Sandbox permissions (must stay in step with ARCHITECTURE.md §Sandbox).
- The licence (D-012), the application ID and the project name (D-013).
- Anything touching keymap or macro storage (D-015).
- Accepted decisions, unless their stated reversal condition has fired.
- The public key-name vocabulary once profiles exist in the wild.
