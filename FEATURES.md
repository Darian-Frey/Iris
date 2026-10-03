# Features

## Target users

Owners of a Tecware Phantom RGB keyboard on the EVision `320f:5064` controller who run desktop Linux and want per-key lighting, application-aware profiles and a real editor. The reference user is the author, on Linux Mint 22.3 / Cinnamon (X11), with the 88-key ISO-UK board.

## Out of scope

- **Host-driven animation on stock firmware.** The V1 protocol has no direct (non-saving) mode and every colour write is assumed to reach flash (D-001, AV-004). Custom frame-by-frame effects are not offered.
- **Layered effects.** The firmware runs one mode at a time; a custom per-key map cannot sit under a reactive ripple.
- **Custom firmware.** No QMK port, no reflashing, no bootloader interaction (D-001).
- **Other operating systems.** Linux only.
- **Other keyboards.** The protocol crate is written so other EVision V1 boards could be added, but none is supported or tested (see candidates).
- **Any command not on the allow-list** in PROTOCOL.md §Command allow-list (D-005).

## Features

### Device and protocol

### F-001 Device discovery and access
**Priority:** Must
**Acceptance:**
- Finds the keyboard by scanning `/sys/class/hidraw/*/device/uevent` for `HID_ID` `0003:0000320F:00005064` and selects the node whose report descriptor declares usage page `0xFF1C`
- Opens that hidraw node without root, given the udev rule from F-025
- Never opens or detaches the boot-keyboard interface; typing is uninterrupted at all times
- Verifies `bcdDevice` and the `55 aa` capability magic before permitting any write (AV-007)
- Survives unplug/replug and hidraw renumbering without a restart
**Status:** Not started
**Notes:** D-003. Discovery logic is proven in `tools/phantom_probe.py`.

### F-002 Built-in lighting mode control
**Priority:** Must
**Acceptance:**
- Sets any of the modes the keyboard itself advertises in its capability block (19 on the reference board)
- Sets brightness, speed, direction, random-colour flag and mode colour where the mode supports them
- The valid numeric ranges for brightness and speed are established on hardware and recorded in PROTOCOL.md before release (they differ between the two prior implementations)
**Status:** Not started
**Notes:** PROTOCOL.md §Parameters, §Modes.

### F-003 Per-key custom colour maps
**Priority:** Must
**Acceptance:**
- Uploads a full 88-key colour map in bulk packets (≤ 54 bytes of colour data each)
- Updates a single key with one data packet inside one transaction
- Every key in `data/phantom_iso_uk_leds.toml` lights the correct physical key, including the two ISO keys (AV-006)
**Status:** Not started
**Notes:** Single-key colour via the V1 path confirmed on hardware with OpenRGB, 2026-09-20.

### F-004 State read-back
**Priority:** Must
**Acceptance:**
- Reads active profile, all three per-profile configuration blocks and the custom colour map
- GUI and CLI show the keyboard's actual state on start, not an assumed one
**Status:** Not started
**Notes:** Commands `0x03`, `0x05`, `0x10` all answered correctly on hardware, 2026-09-20.

### F-005 Onboard profile slot selection
**Priority:** Should
**Acceptance:**
- Switches between the three onboard slots
- Implemented strictly as read-modify-write of the capability block; never writes a canned block (AV-016)
- Enabled only after the `0x04` write has been verified on this firmware (D-005)
**Status:** Not started
**Notes:** Gated. The prior-art packet for this contains another keyboard's device-specific bytes.

### F-006 Polling rate
**Priority:** Could
**Acceptance:**
- Sets 125 / 250 / 500 / 1000 Hz and reads the value back
**Status:** Not started
**Notes:** Parameter `0x0F`; unverified on this firmware.

### Profiles and automation

### F-007 Profile files
**Priority:** Must
**Acceptance:**
- A profile is one TOML file: mode and parameters, or a per-key colour map keyed by key name, plus optional match rules
- Files live under the app's config directory, are hand-editable and diff cleanly in git
- Invalid files are rejected with a line-level error and never partially applied
**Status:** Not started
**Notes:** D-011.

### F-008 Application-aware profile switching (X11)
**Priority:** Must
**Acceptance:**
- Event-driven on `_NET_ACTIVE_WINDOW`; no polling
- Rules match on `WM_CLASS` instance/class and window title (substring or regex), with explicit priority and a fallback profile
- Focus changes are debounced so rapid alt-tabbing produces at most one write burst (AV-013)
- Works for native, Wine and Steam/Proton windows
**Status:** Not started
**Notes:** D-006, D-007. Process-path matching is deliberately absent; see AV-011.

### F-009 Idle, lock and suspend behaviour
**Priority:** Should
**Acceptance:**
- Dims or switches off after a configurable idle time; restores on activity
- Switches off on screen lock, restores on unlock
- Re-discovers the device and re-applies the correct profile after resume (AV-014)
**Status:** Not started

### F-010 Night schedule
**Priority:** Could
**Acceptance:**
- Time-window rule that caps brightness or selects a profile
**Status:** Not started

### F-011 Event profiles
**Priority:** Could
**Acceptance:**
- A D-Bus method applies a named profile for N seconds then reverts, for use by scripts (build finished, mic muted)
- Subject to the same rate limit as everything else (F-012)
**Status:** Not started

### F-012 Write minimisation and wear accounting
**Priority:** Must
**Acceptance:**
- The daemon holds a shadow copy of device state and sends only the bytes that differ
- Applying an identical profile sends zero write packets
- A global rate limit caps write transactions; excess requests coalesce to the latest
- A persistent counter records total write transactions and is visible in GUI and CLI
**Status:** Not started
**Notes:** AV-004. This is a Must because the flash-or-RAM question is open.

### F-029 Lighting state snapshot and restore
**Priority:** Should
**Acceptance:**
- Saves the three configuration blocks and custom maps to a file before Iris first writes to a device, and can restore them
**Status:** Not started

### Graphical interface

### F-013 ISO-UK visual keyboard editor
**Priority:** Must
**Acceptance:**
- Draws the 88-key UK layout with real legends and correct key sizes (ISO Return, short left Shift)
- Click to select, colour picker to paint, live preview of the map before applying
- Apply is explicit; nothing is written while the user is still painting
**Status:** Not started

### F-014 Semantic selection and painting tools
**Priority:** Should
**Acceptance:**
- One-click groups: WASD, arrows, F-row, number row, modifiers, navigation cluster, alphas
- Drag/lasso select, gradient across a selection, horizontal mirror, saved palettes
**Status:** Not started

### F-015 Effect controls
**Priority:** Must
**Acceptance:**
- Lists only modes the keyboard advertises; shows only the controls each mode honours
**Status:** Not started

### F-016 Profile manager and rule editor
**Priority:** Must
**Acceptance:**
- Create, duplicate, rename, delete profiles; edit match rules with a "pick a window" helper that reads the clicked window's `WM_CLASS` and title
- Shows which rule matched the currently focused window
**Status:** Not started

### F-017 Colour legend
**Priority:** Could
**Acceptance:**
- Optional per-profile legend (colour → meaning) shown in the GUI and as a small always-on-top panel
**Status:** Not started

### F-018 Keybind import
**Priority:** Could
**Acceptance:**
- Generates a colour map from a simple `key = category` text file; categories map to palette colours
**Status:** Not started
**Notes:** Game-specific config parsers are a candidate, not a commitment.

### F-019 Colour calibration
**Priority:** Should
**Acceptance:**
- Per-channel gain so that requested white looks white on the hardware; applied in the daemon so all clients benefit
- GUI preview approximates the calibrated output
**Status:** Not started

### F-020 Tray icon and quick switch
**Priority:** Should
**Acceptance:**
- StatusNotifierItem tray entry under Cinnamon: current profile, manual override, pause automation, open editor
**Status:** Not started

### F-021 Usage heatmap
**Priority:** Could
**Acceptance:**
- Opt-in, off by default. Stores per-key press **counts only**: no sequences, no timestamps, no per-application breakdown (AV-015)
- Renders counts as a colour map in the GUI and optionally on the keyboard
**Status:** Not started
**Notes:** Key events are readable as HID report ID 1 on the same hidraw node.

### Interfaces and packaging

### F-022 D-Bus API
**Priority:** Must
**Acceptance:**
- Session-bus service under the name `io.github.darian_frey.Iris`
- Methods for: list/apply profile, set mode and parameters, set key colours, read state, pause/resume automation, timed event profile, write counter
- Signals for profile changed, device connected/disconnected
- GUI and CLI use only this API; neither touches the device
**Status:** Not started
**Notes:** D-008.

### F-023 `irisctl` command-line client
**Priority:** Must
**Acceptance:**
- Everything the D-Bus API offers, scriptable, with `--json` output
- Runnable from the host as `flatpak run --command=irisctl io.github.darian_frey.Iris …`
**Status:** Not started

### F-024 Flatpak packaging
**Priority:** Must
**Acceptance:**
- Single manifest builds daemon, CLI and GUI offline from vendored cargo sources
- Installs per-user with `flatpak-builder --user --install`
- Desktop file, icon and AppStream metainfo validate
- Sandbox permissions are exactly those listed in ARCHITECTURE.md §Sandbox
**Status:** Not started
**Notes:** D-009, D-010.

### F-025 Host setup helper
**Priority:** Must
**Acceptance:**
- On a permission failure the GUI and CLI print the exact udev rule and the three commands to install it (AV-010)
- The rule file ships in `data/` and inside the Flatpak at a documented path
**Status:** Not started

### F-026 Background autostart
**Priority:** Should
**Acceptance:**
- User can enable "start at login"; implemented through the XDG Background portal, which launches `irisd` inside the sandbox
- GUI starts the daemon on demand if it is not running (AV-012)
**Status:** Not started

### Input configuration (gated)

### F-027 Key remapping with revert-on-timeout
**Priority:** Could
**Acceptance:**
- Remap editor; after applying, the change reverts automatically unless confirmed within 10 s using the keyboard itself
- Factory keymap is captured and restorable
**Status:** Not started
**Notes:** Blocked on D-015: requires USB captures of the vendor software against this exact firmware. AV-003.

### F-028 Macros
**Priority:** Could
**Acceptance:**
- To be defined after captures exist
**Status:** Not started
**Notes:** Blocked on D-015. No prior implementation anywhere.

## Candidate features (uncommitted)

- Wayland focus backends (KDE, GNOME extension, wlroots foreign-toplevel) behind the existing trait (D-006).
- Support for other EVision V1 boards (`0c45:652f` Phantom/GMMK, several Redragon models), each needing its own LED map and hardware owner.
- A non-saving direct mode, if vendor-software captures reveal one on this firmware.
- Game-specific keybind importers.
- A sibling project porting QMK to the board's SN32F268, which would lift the animation limits entirely.
