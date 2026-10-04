# Attack Vectors

Project-specific failure modes the project must be resilient against.
Grouped by category. Each vector lists detection method and severity.
Severity: Critical (must hold) | Major (regression on release blocks) | Minor (track only).

No production code exists yet, so most detections are specified but not implemented. Entries move to implemented as the named tests land; note the event in History.

## Protocol safety

### AV-001 Sending a command whose meaning is unverified on this firmware
**Severity:** Critical
**Description.** The keyboard answers unknown or misused commands with a normal-looking reply and status `0x00`, so a harmful write is indistinguishable from a harmless read at the time it is sent. The cost of a wrong guess is a corrupted keymap or configuration block on a device with no known-good image to restore.
**Detection.** Partly implemented. `iris-proto` exposes a closed `Command` enum and no raw-command API; requests are built only through typed constructors. Tests `allow_list_matches_protocol`, `from_byte_accepts_only_the_allow_list`, `forbidden_commands_are_absent` and `every_constructible_request_is_allow_listed`. Still planned: `tools/` scripts import the same list (IMP-001).
**Related decisions.** D-005, D-004, D-014.
**History.** Identified 2026-09-20 from BUG-001. Enum and tests landed in `iris-proto` 2026-10-03.

### AV-002 Cross-dialect command collision
**Severity:** Critical
**Description.** EVision V2 command numbers overlap V1 write commands (V2 `0x0A` read colours = V1 `0x0A` write keymap header). Code or advice written for V2 boards is actively dangerous here.
**Detection.** Not implemented (planned: same enum test as AV-001; plus manual review rule that no code is adapted from OpenRGB's `EVisionV2KeyboardController`).
**Related decisions.** D-004.
**History.** Identified 2026-09-20; realised once as BUG-001.

### AV-003 Keymap corruption
**Severity:** Critical
**Description.** A wrong keymap write can shift or disable keys, including the ones needed to recover. Known-good ISO packets exist only for different hardware and firmware.
**Detection.** Not implemented (feature gated by D-015; when built: simulated-device tests, capture-derived golden packets, revert-on-timeout, verified factory reset).
**Related decisions.** D-015.
**History.** dokutan's README warns of differing firmware versions and broken ISO remapping on some boards.

### AV-004 Flash wear from frequent writes
**Severity:** Major
**Description.** Colour and parameter writes are assumed to reach non-volatile memory with finite endurance. Focus-driven switching could issue thousands of writes a day.
**Detection.** Not implemented (planned: daemon test that applying an identical profile yields zero write packets against the simulated device; rate-limiter test; persistent write counter surfaced in GUI/CLI for manual review).
**Related decisions.** D-001.
**History.** Flash-vs-RAM is OPEN (PROTOCOL.md §9 item 2). If writes prove volatile, downgrade to Minor.

### AV-005 Unterminated transaction
**Severity:** Major
**Description.** A begin (`0x01`) without its end (`0x02`), through a crash, timeout or unplug mid-write, may leave the firmware waiting or discard a partial update. Behaviour is unknown.
**Detection.** Not implemented (planned: transaction guard type whose drop sends end; fault-injection tests on the simulated device; one deliberate hardware observation once the device core exists). The simulated keyboard exists (2026-10-04): it models an abandoned transaction as applying nothing, flags nested begins and stray ends, and can drop replies or unplug mid-transaction, so the guard's tests can be written against it.
**Related decisions.** D-003.

### AV-016 Overwriting device-specific bytes with a canned block
**Severity:** Critical
**Description.** Prior art changes the active profile by writing a fixed 44-byte capability block captured from a different keyboard. Bytes 4–11 of that block differ on this board and their meaning is unknown.
**Detection.** Partly implemented. `iris-proto`'s `Capabilities` and `ConfigBlock` can only be built by parsing a device reply and keep the raw bytes whole; neither can be serialised for writing. Still planned with `0x04` (F-005): "patch byte N of a block previously read", with a test that all other bytes round-trip unchanged.
**Related decisions.** D-005.
**History.** Found 2026-09-20 comparing dokutan's `_data_profile` with this board's `0x03` reply.

## Device identity and access

### AV-007 Firmware or hardware variant mismatch
**Severity:** Major
**Description.** The Phantom ships with at least two controllers (`0c45:652f`, `320f:5064`) and firmware revisions. Offsets verified on one may be wrong on another.
**Detection.** Not implemented (planned: identity check of VID/PID, `bcdDevice` and `55 aa` magic before any write; unknown combinations are read-only until the user explicitly opts in).
**Related decisions.** D-005.

### AV-008 Reply desynchronisation
**Severity:** Major
**Description.** Key-press, media-key and mouse reports (IDs 1, 2, 3, 5) arrive on the same hidraw node as vendor replies. Taking the next read as "the reply" will misparse a keystroke as device state, and the real reply will then be misattributed to the following request.
**Detection.** Partly implemented. `Reply::parse` rejects other report IDs as `ForeignReport` and `Reply::answers` requires bytes 1–6 to echo the request; tests `foreign_reports_are_rejected` and `interleaved_reports_leave_the_reply_intact` in `iris-proto` (2026-10-03). Still planned: the same test against the transport in `iris-device`. The probe script already filters by report ID.
**Related decisions.** D-003.

### AV-009 Concurrent access by another tool
**Severity:** Minor
**Description.** OpenRGB or a second `irisd` talking to the same node will interleave transactions and invalidate the shadow state.
**Detection.** Not implemented (planned: advisory `flock` on the node; D-Bus name ownership prevents a second daemon; periodic read-back compares device state with shadow state and resynchronises).

### AV-010 Missing host permission
**Severity:** Major
**Description.** Without the udev rule the node is root-only and the Flatpak cannot fix that. A silent failure looks like "Iris does nothing".
**Detection.** Not implemented (planned: distinct permission-denied error mapped to a dialog and CLI message containing the exact rule and commands).
**Related decisions.** D-009.
**History.** Hit during reconnaissance: `/dev/hidraw*` were `crw------- root`.

## Sandbox

### AV-011 Host processes are invisible from the sandbox
**Severity:** Minor
**Description.** `_NET_WM_PID` cannot be resolved to an executable inside the Flatpak's PID namespace. Any rule type that depends on it would silently never match.
**Detection.** Manual review: the rule schema has no process field (D-007).
**Related decisions.** D-007, D-009.

### AV-012 Daemon not running
**Severity:** Minor
**Description.** There is no systemd unit. If autostart is off and the GUI is closed, profiles stop switching, with no indication.
**Detection.** Not implemented (planned: GUI and CLI detect an unowned bus name, start the daemon on demand and say so; tray icon presence is the user-visible liveness signal).
**Related decisions.** D-009.

## Automation

### AV-013 Focus thrash causing a write storm
**Severity:** Major
**Description.** Rapid alt-tabbing, transient popups and tooltips generate bursts of focus events, each of which could trigger a full profile write.
**Detection.** Not implemented (planned: debounce test feeding a synthetic burst and asserting one write transaction; ignores override-redirect and transient windows).
**Related decisions.** D-006.

### AV-014 Stale state after suspend or replug
**Severity:** Major
**Description.** After resume or replug the hidraw node may be renumbered and the keyboard may have reverted to a stored profile. A daemon that trusts its old file descriptor and shadow state will write to nothing, or diff against fiction.
**Detection.** Not implemented (planned: on `PrepareForSleep(false)` and on hotplug, re-run discovery, re-read device state, rebuild shadow state, then re-evaluate rules; simulated-device test for each).

## Privacy

### AV-015 The heatmap becomes a keylogger
**Severity:** Major
**Description.** The daemon can read every key press from report ID 1. Storing order, timing or per-application data would create a keystroke log in a world-readable-by-user state directory.
**Detection.** Not implemented (planned: the storage type is a fixed array of counters with no other fields; code review gate on any change to it; feature off by default; trace logging never includes key identities).

## Data

### AV-006 Wrong LED index for a key
**Severity:** Minor
**Description.** The LED map is derived from an ANSI-named table for a sibling board. The two ISO-specific keys (LEDs 64 and 105) are unverified.
**Detection.** Manual: light each LED in turn and tick it off against the physical key; record the result in PROTOCOL.md §8. A "walk the keyboard" test mode in `irisctl` is planned to make this repeatable.
**History.** LED 59 = J confirmed on hardware 2026-09-20.
