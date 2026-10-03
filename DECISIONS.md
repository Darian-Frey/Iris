# Decisions

Append-only log of significant design decisions.
Each entry: D-NNN, with Decided and Recorded dates (ISO 8601), status, context, alternatives, decision, consequences, and reversal conditions.
Status vocabulary: Proposed | Accepted | Superseded by D-NNN | Deprecated.

### D-001 Stay on stock firmware
**Decided:** 2026-09-19
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-19)
**Related:** F-002, F-003, AV-004, PROTOCOL.md §3

**Context.** The keyboard's stock firmware offers built-in effects and a stored per-key colour map, but no live frame streaming. Replacing the firmware would remove that ceiling.

**Options.**
- **A. Control the stock firmware over its vendor HID protocol.** Low risk, protocol already reverse-engineered by two projects. Ceiling: no host-driven animation, one mode at a time.
- **B. Port QMK to the board (SonixQMK).** True per-key streaming and full remapping. Rejected for now: the SonixQMK database lists the Phantom 87 (SN32F268) with no existing port, so it means opening the board, mapping the key and LED matrices by hand, and accepting brick risk with no stock firmware image to fall back on.

**Decision.** Option A.

**Consequences.**
- Custom animations and layered effects are out of scope (FEATURES.md §Out of scope).
- The project's value is in profiles, automation and the editor, not in effects.
- Flash wear becomes a design constraint (F-012).

**Reversal conditions.** Revisit if (a) a working SonixQMK port for this exact PCB appears, with a recoverable stock image, or (b) vendor-software captures reveal a non-saving direct mode, which would not reverse this decision but would lift most of its cost.

### D-002 Daemon, GUI and CLI as separate programs
**Decided:** 2026-09-19
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-19)
**Related:** F-022, F-023, D-008, ARCHITECTURE.md

**Context.** Application-aware switching must run with no window open. Something long-lived has to own the device.

**Options.**
- **A. Single GUI application that also does the background work.** Simplest; rejected because closing the window stops automation, and scripts have nothing to talk to.
- **B. Rust daemon owning the device; C++/Qt 6 GUI and Rust CLI as clients.** Chosen.
- **C. All-Rust, GUI via cxx-qt or a Rust-native toolkit.** One toolchain. Rejected: the editor is the most demanding UI work in the project and plain Qt Widgets/C++ is the best-trodden path for it; the D-Bus boundary makes the language split cheap.

**Decision.** Option B. Exactly one process (`irisd`) opens the hidraw node.

**Consequences.**
- Two toolchains (cargo and CMake) in one repository and one Flatpak manifest.
- The D-Bus API is a real contract and must be designed before the GUI.
- Device access is serialised in one place, which makes F-012 tractable.

**Reversal conditions.** Revisit option C if maintaining the C++/Rust split costs more than it saves, for example if the GUI ends up duplicating protocol knowledge that lives in `iris-proto`.

### D-003 Raw hidraw on interface 1; no libusb, no hidapi
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** F-001, AV-005, AV-008, PROTOCOL.md §1

**Context.** dokutan's tool uses libusb, which must detach the kernel driver and leaves the keyboard dead until replugged. The reference board's interface 1 has no OUT endpoint, so output reports are control transfers.

**Options.**
- **A. libusb.** Rejected: interrupts typing, needs driver detach/re-attach, and must special-case control-transfer mode.
- **B. hidapi (hidraw backend).** Works; rejected because its enumeration leans on udev, which is unreliable inside a Flatpak sandbox, and it is a C dependency to vendor.
- **C. Open `/dev/hidrawN` directly; enumerate through `/sys/class/hidraw`.** Chosen. The kernel turns `write()` into SET_REPORT automatically. Proven by `tools/phantom_probe.py`.

**Decision.** Option C, standard library only.

**Consequences.**
- Typing is never interrupted and no replug is ever needed.
- The reader must filter by report ID, because key-press reports share the node (AV-008).
- Linux-only by construction.

**Reversal conditions.** Revisit if a supported board exposes its vendor channel in a way hidraw cannot reach, or if a non-Linux target is ever adopted.

### D-004 Speak EVision V1 only; V2 commands are prohibited
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** AV-002, BUG-001, D-005, PROTOCOL.md §3

**Context.** OpenRGB implements a newer V2 dialect with a direct mode, which was tempting. The reference board reports the V1 magic and is registered under V1. V2 command numbers collide with V1 write commands, and a V2 "read" sent during reconnaissance turned out to be a V1 keymap-header write (BUG-001).

**Options.**
- **A. Probe V2 commands to see what answers.** Rejected: the status byte does not distinguish understood from ignored, and the cost of a wrong guess is a corrupted keymap.
- **B. V1 only.** Chosen.

**Decision.** Option B.

**Consequences.** No direct mode. Any future dialect question is answered by captures, not by probing.

**Reversal conditions.** Vendor-software captures against this firmware showing V2-style commands in use.

### D-005 Command allow-list enforced in the protocol crate
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** AV-001, AV-007, AV-016, BUG-001, F-005, PROTOCOL.md §Command allow-list

**Context.** BUG-001 showed that "it looked like a read" is not a safety argument.

**Options.**
- **A. Discipline and code review only.** Rejected: that is what failed.
- **B. The command byte is a closed Rust enum in `iris-proto`; there is no API that accepts an arbitrary command number; adding a variant requires a PROTOCOL.md status of HW or an explicit gated-experiment note.** Chosen.

**Decision.** Option B.

**Consequences.**
- Experiments with new commands need a deliberate code change, which is the point.
- `0x04` (active profile) stays out until PROTOCOL.md §9 item 4 is done.

**Reversal conditions.** None foreseen for the principle. Individual commands join the list as they are verified.

### D-006 X11 focus detection first, behind a trait
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** F-008, AV-013

**Context.** The author runs Cinnamon 6.6 on Mint 22.3, which is an X11 session; Cinnamon's Wayland session is experimental. Wayland has no portable focus API.

**Options.**
- **A. X11 only, hard-wired.** Rejected: cheap to abstract now, expensive later.
- **B. X11 backend behind a `FocusSource` trait.** Chosen.
- **C. X11 plus Wayland backends from the start.** Rejected: three compositor-specific backends with nobody to test them.

**Decision.** Option B, using `_NET_ACTIVE_WINDOW` property events.

**Consequences.** Wayland users get manual profile switching only, until a backend is written.

**Reversal conditions.** The author moves to a Wayland session, or Cinnamon makes Wayland its default.

### D-007 Match rules on WM_CLASS and title, not on process
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** F-008, F-016, AV-011, D-009

**Context.** `_NET_WM_PID` gives a host PID. Inside a Flatpak the daemon has its own PID namespace, so `/proc/<pid>/exe` does not resolve to the focused application.

**Options.**
- **A. Match on executable path.** Rejected: impossible from inside the sandbox without `--share`-level escapes.
- **B. Match on `WM_CLASS` and window title only.** Chosen. Both are readable over the X11 socket.

**Decision.** Option B.

**Consequences.** Applications that share a `WM_CLASS` (some Wine/Proton titles) need a title pattern to tell them apart. The rule editor's window picker (F-016) exists to make this painless.

**Reversal conditions.** Iris stops being distributed as a Flatpak, or a portal for window identity appears.

### D-008 D-Bus session service under the application ID as the only IPC
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** F-022, F-023, D-002, D-009

**Context.** GUI, CLI and host scripts all need to reach the daemon, across a sandbox boundary.

**Options.**
- **A. Unix socket in the runtime directory.** Rejected: the per-app runtime directory is awkward to reach from host scripts.
- **B. D-Bus session name equal to the Flatpak app ID.** Chosen. Flatpak lets an app own its own ID with no extra permission, and any host tool (`busctl`, `gdbus`, a shell script) can call it.

**Decision.** Option B; `zbus` in Rust, QtDBus in the GUI.

**Consequences.** The API is introspectable and scriptable for free. Anything on the user's session bus can drive the keyboard, which is acceptable for a lighting controller; F-012's rate limit protects the hardware regardless of caller.

**Reversal conditions.** A requirement for access control finer than "the user's own session".

### D-009 Distribute as a single Flatpak with the daemon inside the sandbox
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley
**Related:** F-024, F-025, F-026, D-007, D-010, AV-010, AV-011, AV-012

**Context.** The author wants a Flatpak for installation on his desktop. Flatpaks cannot install udev rules or systemd units.

**Options.**
- **A. Native packages (deb) with a systemd user service.** Cleanest integration; rejected as the primary route because it is not what was asked for and ties the project to one distribution.
- **B. Flatpak for the GUI, native daemon outside.** Rejected: two install procedures.
- **C. One Flatpak containing `irisd`, `irisctl` and the GUI; autostart through the XDG Background portal; udev rule as a one-time documented host step.** Chosen.

**Decision.** Option C. App ID `io.github.darian_frey.Iris`.

**Consequences.**
- One host-side manual step (udev rule) that the application must diagnose and explain (F-025).
- No process-based matching (D-007).
- The daemon is started by the portal's autostart entry or on demand by the GUI, not by systemd (AV-012).

**Reversal conditions.** Sandbox constraints block a Must feature with no portal-based workaround.

### D-010 Sandbox device permission is `--device=all`
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** F-024, D-009

**Context.** The daemon needs `/dev/hidrawN`. Flatpak has no hidraw-specific permission, and Mint 22.x ships a Flatpak 1.14 series that predates the finer-grained device options.

**Options.**
- **A. `--device=all`.** Broad, but it is what every hidraw-using Flatpak does, OpenRGB included. Chosen.
- **B. Wait for or require a newer Flatpak with narrower device permissions.** Rejected: not available on the reference platform, and it is not established that any of them covers hidraw.

**Decision.** Option A. Actual access is still bounded by the host udev rule, which names one VID/PID.

**Consequences.** The permission will be flagged by Flathub review and by permission viewers; the README must explain it.

**Reversal conditions.** A Flatpak release on the reference platform offers a permission that covers hidraw nodes specifically.

### D-011 Profiles are TOML files
**Decided:** 2026-09-19
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-19)
**Related:** F-007

**Context.** Profiles need to be shareable, hand-editable and version-controllable.

**Options.**
- **A. SQLite or a binary store.** Rejected: opaque, poor diffs.
- **B. JSON.** Rejected: no comments, noisy for 88 key entries.
- **C. One TOML file per profile, keys addressed by name.** Chosen.

**Decision.** Option C.

**Consequences.** Key names become a public, stable vocabulary; they are defined once in `data/phantom_iso_uk_leds.toml`.

**Reversal conditions.** Profiles grow structure TOML expresses badly (deeply nested rules).

### D-012 Licence is GPL-3.0-or-later
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** README.md §Acknowledgements

**Context.** Iris's protocol knowledge and its LED index table derive from reading dokutan/rgb_keyboard (GPL-3.0) and OpenRGB (GPL-2.0-or-later). Protocol facts are not code, but the LED table is a direct transformation of dokutan's data.

**Options.**
- **A. Permissive (MIT/Apache).** Rejected: uncertain footing given the derived table, and it would prevent borrowing prior-art code later.
- **B. GPL-3.0-or-later.** Chosen. Compatible with both sources.

**Decision.** Option B. This is a cautious engineering choice, not legal advice.

**Consequences.** Code from either prior-art project may be adapted with attribution.

**Reversal conditions.** The LED table is independently re-derived on hardware and no prior-art code has been incorporated, and there is a concrete reason to want a permissive licence.

### D-013 Project name is Iris
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley
**Related:** D-009

**Context.** The name should reflect what the program does and fit the author's mythological naming convention.

**Options.**
- **A. Iris.** Goddess of the rainbow and messenger of the gods: colour, plus a daemon that carries messages to the device. Chosen.
- **B. Hemera, Theia, Arke, Selas, Lampas.** Considered. Hemera is more strictly primordial; Theia collides with the Eclipse IDE; the rest say less about function.

**Decision.** Option A. Binaries `irisd`, `irisctl`, `iris`. App ID `io.github.darian_frey.Iris` (hyphen in the GitHub user name becomes an underscore, per reverse-DNS ID rules).

**Consequences.** A common name; the repository may need a qualifier such as `iris-rgb` for discoverability.

**Reversal conditions.** A naming conflict on Flathub or a trademark objection.

### D-014 The specification is PROTOCOL.md with per-fact verification tags; no CLAIMS.md
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** PROTOCOL.md, AV-001, D-005

**Context.** Iris is not research, but its spec consists of reverse-engineered assertions of varying reliability, some captured on different hardware.

**Options.**
- **A. SPEC.md plus a CLAIMS.md for protocol assertions.** Rejected: splits one body of knowledge across two files.
- **B. Domain-named PROTOCOL.md where every fact is tagged HW / SRC / OPEN, with an experiments list.** Chosen.

**Decision.** Option B.

**Consequences.** The tags are load-bearing: a command may join the allow-list only when tagged HW or explicitly gated (D-005).

**Reversal conditions.** The project starts making empirical claims beyond the protocol (for example measured flash endurance).

### D-015 Key remapping and macros are gated on vendor-software captures
**Decided:** 2026-09-20
**Recorded:** 2026-09-20
**Status:** Accepted
**Authors:** Shane Hartley (with Claude analysis 2026-09-20)
**Related:** F-027, F-028, AV-003, BUG-001

**Context.** dokutan's ISO keymap packets were captured on `0c45:652f` firmware 1.03 and he warns that firmware versions may differ. This board is `320f:5064` firmware 1.02 with a different interface layout. A wrong keymap write can leave a keyboard that cannot type its own recovery.

**Options.**
- **A. Try dokutan's packets and see.** Rejected.
- **B. No keymap or macro command is implemented until USB captures of the Tecware software against this exact keyboard exist, and a factory-reset procedure has been confirmed to work.** Chosen.

**Decision.** Option B.

**Consequences.** F-027 and F-028 sit in the final roadmap phase and may never ship. Host-side remapping (keyd, xmodmap) remains available to the user and is arguably the better tool.

**Reversal conditions.** Captures exist and the factory reset is verified.
