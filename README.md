> **Status:** Active
> **Provenance:** Shane Hartley (author, hardware testing); Claude (protocol research, source analysis of prior art, initial documentation 2026-09-20)
> **Last reviewed:** 2026-09-20
> **Why this status:** Reconnaissance complete (protocol identified and validated on hardware). No production code yet; Phase 1 (device core) is next.

# Iris

Iris is a Linux control application for the Tecware Phantom RGB mechanical keyboard (88-key ISO-UK, EVision controller, USB `320f:5064`). It is for people who want proper per-key lighting control, application-aware profiles and a usable interface on Linux, where the vendor ships nothing and the existing open tools are either a bare CLI or a generic driver that labels keys "LED 59". Iris pairs a background daemon (the messenger) with a Qt 6 editor that draws the real UK layout, and it treats the keyboard's flash memory and keymap as things that can be damaged, so every command it sends is one whose meaning has been confirmed on this firmware.

Named for the Greek goddess of the rainbow, who was also the gods' messenger.

## Quick Start

There is no installable build yet. What exists today is the reconnaissance tooling:

```bash
# 1. Host permission (once). A Flatpak cannot install udev rules.
sudo cp data/60-iris-keyboard.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger
# replug the keyboard, then confirm: getfacl /dev/hidraw1

# 2. Read-only probe (sends read commands only)
python3 tools/phantom_probe.py /dev/hidraw1
```

Intended install path once Phase 1 lands (see [BUILD.md](BUILD.md), unverified until first build):

```bash
flatpak-builder --user --install --force-clean build-dir io.github.darian_frey.Iris.yml
flatpak run io.github.darian_frey.Iris
```

## Build requirements

- Linux Mint 22.3 / Cinnamon 6.6 (X11 session) is the reference platform.
- Flatpak and flatpak-builder; KDE Platform/SDK 6.x runtime; `org.freedesktop.Sdk.Extension.rust-stable`.
- Native development: Rust (rustup, stable), CMake ≥ 3.25, Qt 6.4+.

Details and exact commands: [BUILD.md](BUILD.md).

## Project structure (planned)

```
iris/
├── crates/
│   ├── iris-proto/      # packet building/parsing, command allow-list (no I/O)
│   ├── iris-device/     # hidraw discovery and transactions
│   ├── irisd/           # daemon: profiles, focus rules, D-Bus service
│   └── irisctl/         # CLI client of the D-Bus API
├── gui/                 # Qt 6 (C++) editor, client of the D-Bus API
├── data/                # LED map, udev rule, desktop/metainfo files
├── tools/               # reconnaissance scripts (read-only probe)
├── flatpak/             # manifest and generated cargo sources
└── *.md                 # this documentation set
```

## Documentation

- [Features](FEATURES.md)
- [Roadmap](ROADMAP.md)
- [Architecture](ARCHITECTURE.md)
- [Decisions](DECISIONS.md)
- [Protocol specification](PROTOCOL.md) (the project's SPEC)
- [Attack vectors](ATTACK_VECTORS.md)
- [Bugs](BUGS.md)
- [Improvements](IMPROVEMENTS.md)
- [Build instructions](BUILD.md)
- [Changelog](CHANGELOG.md)
- [AI handoff](CLAUDE.md)

## Acknowledgements

The protocol knowledge in this project comes from reading two prior open-source implementations: [dokutan/rgb_keyboard](https://github.com/dokutan/rgb_keyboard) (GPL-3.0) and OpenRGB's EVision keyboard controller (GPL-2.0-or-later). Neither supports this board well enough on its own, but without them Iris would have started from a Wireshark capture.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) and D-012.
