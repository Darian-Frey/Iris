# Architecture

Describes the system as designed. Rationale lives in DECISIONS.md. Nothing here is built yet; this document becomes descriptive of real code from Phase 1 onward and must be corrected wherever the code diverges.

## System overview

```
                        Flatpak sandbox  (io.github.darian_frey.Iris)
 ┌────────────────────────────────────────────────────────────────────────┐
 │                                                                        │
 │   iris (Qt 6 GUI) ──┐                                                  │
 │                     │  D-Bus session bus                               │
 │   irisctl (CLI) ────┼──────────────►  irisd (daemon)                   │
 │                     │   name = app ID      │                           │
 └─────────────────────┼──────────────────────┼───────────────────────────┘
                       │                      │
   host scripts ───────┘                      ├── FocusSource (X11 socket)
   (busctl, gdbus)                            ├── Session events (D-Bus:
                                              │     Cinnamon ScreenSaver,
                                              │     logind PrepareForSleep)
                                              ├── Profile store (TOML files)
                                              └── iris-device
                                                     │  iris-proto packets
                                                     ▼
                                              /dev/hidrawN  (interface 1,
                                              report ID 0x04)  ── keyboard
```

Data flows one way for control (clients → daemon → device) and back as D-Bus signals (daemon → clients). Only `irisd` touches the device.

## Module responsibilities

**`iris-proto` (Rust library, no I/O).** Builds and parses 64-byte packets: checksum, command enum, addresses for configuration and colour space, capability-block parsing, mode and parameter tables, LED index map. Owns the command allow-list as a closed enum. Pure functions, exhaustively unit-tested, no dependency on the operating system.

**`iris-device` (Rust library).** Finds the hidraw node through sysfs, opens it, performs request/reply exchanges with report-ID filtering and timeouts, wraps writes in begin/end transactions, and guarantees an end packet is sent on every exit path. Performs the identity check (VID/PID, `bcdDevice`, capability magic) and refuses writes if it fails. Watches for hotplug.

**`irisd` (Rust binary).** The only long-lived process. Holds the shadow state of the keyboard and diffs every requested change against it. Applies the rate limiter and write counter. Loads and validates profiles, evaluates match rules against focus events with debouncing, reacts to idle, lock and suspend. Serves the D-Bus API and emits signals. Applies colour calibration on the way out.

**`irisctl` (Rust binary).** Thin client of the D-Bus API. Human and `--json` output. No device or profile logic of its own.

**`iris` GUI (C++ / Qt 6 Widgets).** Keyboard editor, effect controls, profile and rule manager, tray icon. Client of the D-Bus API through QtDBus. Holds no protocol knowledge: it works in key names and colours, never in LED indices or packet bytes. Renders the layout from the same `data/` layout file the daemon uses.

**`data/`.** LED map and key geometry for the ISO-UK board, udev rule, desktop entry, icon, AppStream metainfo.

**`tools/`.** Reconnaissance scripts. Not shipped. Bound by the same allow-list as production code.

## Key invariants

1. **One owner.** Exactly one process holds the hidraw node open. GUI and CLI never open it.
2. **Allow-list.** No packet leaves the process unless its command is a variant of the `iris-proto` command enum (D-005).
3. **Interface 0 is never touched.** No driver detach, no libusb. Typing cannot be interrupted by Iris.
4. **Transactions close.** Every begin (`0x01`) is followed by an end (`0x02`), including on error, timeout and shutdown.
5. **Identity before writes.** No write is sent to a device that has not passed the identity check in this session.
6. **Diff, then write.** No write is issued for bytes that already match the shadow state. Applying the current profile again is a no-op on the wire.
7. **Rate-limited writes.** All write transactions, from any caller, pass through one limiter.
8. **Read-modify-write for shared blocks.** Blocks containing bytes Iris does not understand are read, minimally changed and written back; never synthesised (AV-016).
9. **Replies are matched, not assumed.** A reply is accepted only if its report ID is `0x04` and its command and offset echo the request.
10. **Heatmap stores counts only.** No key sequences or timestamps are ever written to disk.

## Sandbox

Flatpak `finish-args`, and why each exists:

| Permission | Reason |
|------------|--------|
| `--device=all` | hidraw access (D-010) |
| `--socket=x11`, `--share=ipc` | GUI, and reading `_NET_ACTIVE_WINDOW` / `WM_CLASS` / titles for F-008 |
| `--talk-name=org.cinnamon.ScreenSaver` | lock/unlock signals (F-009) |
| `--system-talk-name=org.freedesktop.login1` | suspend/resume signals (F-009) |
| `--talk-name=org.kde.StatusNotifierWatcher` | tray icon (F-020) |
| (implicit) own name `io.github.darian_frey.Iris` | D-Bus API (D-008) |

No network, no home-directory access. Profiles live in the app's own config directory (`~/.var/app/io.github.darian_frey.Iris/config/iris/`), which is reachable from the host for git and hand-editing.

Autostart uses the XDG Background portal, which writes an autostart entry that runs `irisd` inside the sandbox. The udev rule is installed on the host by the user, once.

## Cross-cutting concerns

**Concurrency.** `irisd` is a single-threaded async event loop (tokio current-thread) multiplexing D-Bus, X11 events, timers and the device. Device exchanges are strictly sequential; there is no concurrent access to the hidraw node even within the daemon.

**Error handling.** `iris-proto` returns typed errors and never panics on input. `iris-device` distinguishes *absent*, *permission denied*, *identity mismatch*, *timeout* and *protocol error*; each maps to a distinct D-Bus error so clients can show the right guidance (notably the udev instructions for permission denied).

**Logging.** `tracing` to stderr, captured by the session journal. Packet-level hex logging is available at trace level and is off by default. The heatmap feature never logs key identities.

**Configuration and state.** User configuration and profiles under `$XDG_CONFIG_HOME`; write counter, shadow-state cache and heatmap counts under `$XDG_STATE_HOME`. Both resolve inside the sandbox's per-app directories.

**Testing seam.** `iris-device` exposes a `Transport` trait (`send(&Request)`, `receive(timeout)`) so the daemon can run against a simulated keyboard that implements the V1 protocol in memory. The trait has no raw-bytes method, so the allow-list holds at the transport layer as well. The simulator is `iris_device::sim::SimulatedKeyboard`, compiled only with the `sim` feature (or in `iris-device`'s own tests) and never in a shipped build. It starts from the author's probe capture of 2026-10-04 and reproduces those five replies byte for byte. Where firmware behaviour is unknown it takes the strictest model: writes are staged and applied only at end, writes outside a transaction are ignored, and every misuse is recorded as a `Violation` that tests can assert is absent. These are modelling choices, listed in the module documentation, not protocol facts. All daemon tests use it; hardware is needed only for the experiments in PROTOCOL.md §9.
