# Build

> **Unverified.** Nothing has been built yet. Per the documentation standard this file is normally written after the first successful build; it exists early because the Flatpak target shapes the architecture. Treat every command below as a plan, correct it during Phase 1, and then delete this notice.

## Supported platforms

| Item | Reference |
|------|-----------|
| OS | Linux Mint 22.3 (Ubuntu 24.04 base), x86_64 |
| Desktop | Cinnamon 6.6, X11 session |
| Flatpak | the 1.14 series shipped by Mint 22.x |
| Hardware | Tecware Phantom RGB 88-key, USB `320f:5064` |

Other distributions should work wherever Flatpak and an X11 session exist; none is tested.

## Host prerequisite: udev rule

Required for both native and Flatpak runs. A Flatpak cannot do this for you.

```bash
sudo cp data/60-iris-keyboard.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger
# unplug and replug the keyboard, then:
grep -l 320F /sys/class/hidraw/*/device/uevent      # lists the keyboard's hidraw nodes
getfacl /dev/hidraw1                                # expect a user:<you>:rw- entry
```

`TAG+="uaccess"` grants access to whoever holds the active local session, which is narrower than a world-writable mode.

## Dependencies

### Flatpak build (primary)

```bash
sudo apt install flatpak-builder
flatpak install --user flathub org.kde.Platform//6.x org.kde.Sdk//6.x
flatpak install --user flathub org.freedesktop.Sdk.Extension.rust-stable//XX.08
```

- Pick the newest `org.kde.Platform` 6.x branch on Flathub at first build and **record the exact branch here**.
- The `rust-stable` extension branch must match the freedesktop base of that KDE runtime (each KDE 6.x branch is built on one `XX.08` freedesktop release). `flatpak info org.kde.Sdk//6.x` shows it.
- Offline cargo sources are generated with `flatpak-cargo-generator.py` from the `flatpak/flatpak-builder-tools` repository:

```bash
python3 flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json
```

Regenerate whenever `Cargo.lock` changes (see IMP-002).

### Native development build

```bash
sudo apt install build-essential cmake ninja-build qt6-base-dev pkg-config
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh     # Rust from rustup, not apt
```

Mint 22.x provides Qt 6.4; the GUI must not use APIs newer than that unless the native build is dropped.

## Build commands

### Native

```bash
cargo build --workspace                 # debug
cargo build --workspace --release
cargo test --workspace                  # includes the allow-list test (AV-001)
cmake -S gui -B build/gui -G Ninja -DCMAKE_BUILD_TYPE=RelWithDebInfo
cmake --build build/gui
```

Run natively: start `target/debug/irisd`, then `target/debug/irisctl info` or `build/gui/iris`.

### Flatpak

```bash
flatpak-builder --user --install --force-clean build-dir flatpak/io.github.darian_frey.Iris.yml
flatpak run io.github.darian_frey.Iris                          # GUI
flatpak run --command=irisctl io.github.darian_frey.Iris info   # CLI
flatpak run --command=irisd   io.github.darian_frey.Iris        # daemon in foreground
```

### Manifest draft

```yaml
app-id: io.github.darian_frey.Iris
runtime: org.kde.Platform
runtime-version: '6.x'            # pin at first build
sdk: org.kde.Sdk
sdk-extensions:
  - org.freedesktop.Sdk.Extension.rust-stable
command: iris
finish-args:
  - --device=all                                   # hidraw; see D-010
  - --socket=x11
  - --share=ipc
  - --talk-name=org.cinnamon.ScreenSaver
  - --talk-name=org.kde.StatusNotifierWatcher
  - --system-talk-name=org.freedesktop.login1
build-options:
  append-path: /usr/lib/sdk/rust-stable/bin
  env:
    CARGO_HOME: /run/build/iris-core/cargo
modules:
  - name: iris-core
    buildsystem: simple
    build-commands:
      - cargo --offline build --release --workspace
      - install -Dm755 target/release/irisd   /app/bin/irisd
      - install -Dm755 target/release/irisctl /app/bin/irisctl
      - install -Dm644 data/60-iris-keyboard.rules /app/share/iris/60-iris-keyboard.rules
      - install -Dm644 data/phantom_iso_uk_leds.toml /app/share/iris/layouts/phantom_iso_uk.toml
    sources:
      - type: dir
        path: ..
      - cargo-sources.json
  - name: iris-gui
    buildsystem: cmake-ninja
    subdir: gui
    sources:
      - type: dir
        path: ..
```

The sandbox permissions must match ARCHITECTURE.md §Sandbox exactly; change both together.

## Cross-compilation

Not applicable.

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `Permission denied` opening `/dev/hidrawN` | udev rule missing or keyboard not replugged since it was added | Host prerequisite above |
| Works as root only | Same | Do not run Iris as root; fix the rule |
| AppImages of other tools will not start on Mint 22 | FUSE 2 absent | `sudo apt install libfuse2t64` (only relevant when running OpenRGB for comparison) |
| Keyboard found on a different node after resume | hidraw renumbering | Expected; discovery is by sysfs, never by fixed path |
| Two hidraw nodes match the VID/PID | Interface 0 and interface 1 both appear | Select the one whose report descriptor contains usage page `0xFF1C` |
| OpenRGB and Iris fight | Both hold the node (AV-009) | Close OpenRGB |
