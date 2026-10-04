# Protocol

Authoritative technical reference for talking to the Tecware Phantom RGB (EVision V1 protocol). This is the project's SPEC (D-014).

Every fact carries a verification status, because this is reverse-engineered knowledge and its sources were captured on different hardware:

| Tag | Meaning |
|-----|---------|
| **HW** | Observed on the reference keyboard |
| **SRC** | Taken from prior-art source code (dokutan/rgb_keyboard or OpenRGB); not yet observed here |
| **OPEN** | Unknown or contradictory; needs an experiment or capture |

Update the tag when an experiment settles a fact. Do not promote SRC to HW by inference.

## 1. Reference hardware

| Item | Value | Status |
|------|-------|--------|
| Model | Tecware Phantom RGB, 88-key ISO-UK (serial prefix `P88…UK`) | HW |
| USB ID | `320f:5064`, manufacturer string `SONIX`, product `USB DEVICE` | HW |
| Firmware | `bcdDevice 1.02` | HW |
| MCU | EVision VS11K13A (rebadged Sonix SN32F268), per the SonixQMK keyboard database | SRC |
| Interface 0 | HID boot keyboard, EP `0x81` IN, 64 bytes | HW |
| Interface 1 | HID, EP `0x82` IN, 64 bytes, **no OUT endpoint**; output reports travel as SET_REPORT control transfers, which hidraw `write()` performs transparently | HW |
| Vendor channel | Interface 1, report ID `0x04`, usage page `0xFF1C`, 63-byte input and output reports | HW |
| Other reports on interface 1 | ID 1 NKRO key bitmap (usages `0x04`–`0x70`, 120 bits), ID 2 system control, ID 3 consumer control, ID 5 mouse | HW |

The better-documented sibling is `0c45:652f` (`bcdDevice 1.03`), which has an 8-byte interface 0 endpoint and an interrupt OUT endpoint `0x03` on interface 1. Prior-art data captured there is SRC for this board, never HW.

## 2. Packet format

All packets are 64 bytes. Requests and replies share one layout. **HW**

| Byte | Field |
|------|-------|
| 0 | Report ID, always `0x04` |
| 1–2 | Checksum, little-endian: 16-bit sum of bytes 3–63 |
| 3 | Command |
| 4 | Payload size in bytes |
| 5–6 | Offset / address, little-endian |
| 7 | Request: zero. Reply: status (see below) |
| 8–63 | Payload |

- Every write to the device is followed by exactly one 64-byte reply carrying report ID `0x04`. Reports with other IDs (key presses, media keys) can arrive on the same node in between and must be skipped, not treated as the reply (AV-008). **HW**
- The reply echoes bytes 1–6 of the request. **HW**
- Reply byte 7 is a status code in the V2 dialect (non-zero = error). On this firmware it has only ever been observed as `0x00`, including for a command that was not a valid read, so it must **not** be relied on as proof that a command was understood. **OPEN**
- Whether the firmware validates the request checksum is unknown; dokutan reports it is ignored on the sibling board. Iris always computes it. **OPEN**

Maximum payload is 56 bytes (`0x38`). OpenRGB caps colour writes at 54 (`0x36`), a whole number of RGB triples; Iris does the same. **SRC**

## 3. Commands

| Cmd | Name | Direction | Status | Notes |
|-----|------|-----------|--------|-------|
| `0x01` | Begin transaction | write | SRC | Packet is `04 01 00 01` then zeros |
| `0x02` | End transaction | write | SRC | Packet is `04 02 00 02` then zeros |
| `0x03` | Read capability block | read | HW | Size `0x2C`, offset 0 |
| `0x04` | Write capability block | write | SRC | Used by dokutan to change active profile. See §6 warning |
| `0x05` | Read configuration | read | HW | Size ≤ `0x38`, offset into config space |
| `0x06` | Set parameter | write | SRC (path validated via OpenRGB on HW) | Size = parameter length, offset = parameter address |
| `0x08` | Write keymap data | write | SRC | **Forbidden** (D-015) |
| `0x0A` | Write keymap header | write | SRC | **Forbidden** (D-015). Payload begins `aa 55`. See BUG-001 |
| `0x10` | Read custom colours | read | HW | Size ≤ `0x36`, offset into colour space |
| `0x11` | Write custom colours | write | SRC (validated via OpenRGB on HW) | Size ≤ `0x36`, offset into colour space |

Writes are wrapped: `0x01`, one or more data packets, `0x02`. Reads were answered on hardware without a wrapper. **HW** for reads; **SRC** for the write wrapper.

### Command allow-list

Iris may send **only**: `0x01`, `0x02`, `0x03`, `0x05`, `0x06`, `0x10`, `0x11`. Command `0x04` joins the list only after the experiment in §9 item 4. Everything else is rejected by `iris-proto` at packet-construction time (D-005, AV-001).

### The V2 dialect is not spoken here

OpenRGB also implements a newer "EVision V2" dialect with a non-saving direct mode (`0x12`/`0x13`). Its command numbers **collide with V1 write commands**: V2 `0x0A` is "read custom colours", V1 `0x0A` is "write keymap header". This board reports the V1 magic (`55 aa`; V2 reports `aa 55`) and OpenRGB registers this PID under V1. V2 commands must never be sent (D-004, AV-002). **HW** for the magic; **SRC** for the rest.

## 4. Capability block (command `0x03`)

Reply payload observed on the reference board (all 44 bytes, 2026-10-04; identical to the first 35 bytes recorded on 2026-09-20): **HW**

```
55 aa ff 02 0f 32 64 50 02 01 00 50 00 00 00 00
01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f 11 10 12 14
00 00 00 00 00 00 00 00 00
```

Raw capture: `crates/iris-proto/fixtures/probe-2026-10-04.txt`.

| Payload byte | Meaning | Status |
|--------------|---------|--------|
| 0–1 | Magic `55 aa` | HW |
| 2–9, 11 | Device-specific; differs from the sibling board's `45 0c 2f 65 03 01 00 08` at bytes 4–11 | OPEN |
| 10 | Active profile, zero-based (packet byte 18) | SRC; read as `00` = profile 1 on HW |
| 16–34 | List of supported mode IDs | HW (matches the mode table exactly) |
| 35–43 | Zero on the reference board; meaning unknown | OPEN |

## 5. Configuration space (commands `0x05` / `0x06`)

Three profiles, stride `0x2A` (42 bytes). Address = `profile_index × 0x2A + parameter`. **HW** (promoted from SRC by the author, 2026-10-04): the three reads at `0x00`, `0x2A`, `0x54` returned three coherent blocks, and a `0x38`-byte read at one profile's base runs into the next profile exactly as that profile's own read shows.

### Parameters

| Addr | Parameter | Size | Status |
|------|-----------|------|--------|
| `0x00` | Mode | 1 | HW (read) |
| `0x01` | Brightness | 1 | HW (read) |
| `0x02` | Speed | 1 | HW (read) |
| `0x03` | Direction | 1 | SRC |
| `0x04` | Random-colour flag (`0xFF` on, `0x00` off) | 1 | HW (read) |
| `0x05` | Mode colour R, G, B | 3 | HW (read) |
| `0x0F` | Polling rate: 0 = 125 Hz, 1 = 250, 2 = 500, 3 = 1000 | 1 | SRC |
| `0x11` | "Surmount" mode colour selector | 1 | SRC |

Observed on the reference board, 2026-09-20 (not yet cross-checked against what the keyboard was visibly doing): profile 1 = mode `0x04`, brightness 1, speed 2, colour `96 96 9a`; profiles 2 and 3 = mode `0x01`, brightness 4, speed 2, random on.

Observed again 2026-10-04 (raw capture `crates/iris-proto/fixtures/probe-2026-10-04.txt`): profile 1 = mode `0x04`, brightness 4, speed 0, direction 0, random off, colour `49 00 ff`; profiles 2 and 3 unchanged. The author changed profile 1 with the keyboard's own Fn-key controls between the two reads, so onboard controls write configuration space. With the 2026-10-04 values the author reports the keyboard cycling through colours, brightly. **HW**. This is consistent with mode `0x04` being a colour cycle, but it does not by itself settle the brightness or speed ranges (§9 item 3) or promote the mode names.

- A `0x38`-byte read at a profile's base returns that profile's `0x2A` bytes followed by the first `0x0E` bytes of the next profile, which match the separate read of that profile byte for byte. **HW**
- Block byte `0x13` reads `0xFF` in all three profiles. It is not in the parameter table and its meaning is unknown. **OPEN**
- All other bytes of the three blocks read zero.

Observed again later on 2026-10-04 through `iris-device` (`examples/read_state`, reads only): profile 1 = mode `0x09`, brightness 4, speed 3, direction 0, random-colour byte `0x01`, colour `00 00 00`; profiles 2 and 3 unchanged; byte `0x13` still `0xFF` in all three. The author had changed profile 1 with OpenRGB in between, so OpenRGB wrote these values, including `0x01` at `0x04`, which is neither of the two values in the parameter table. Its meaning is unknown. **OPEN**

**Ranges are contradictory between sources.** **OPEN**
- Brightness: OpenRGB uses 0–4; dokutan uses 0–9 for non-Ajazz boards.
- Speed: OpenRGB uses 0 (fastest) to 5 (slowest); dokutan uses 0–3 and inverts it (`3 − x`).

### Modes

IDs from OpenRGB's V1 controller; all 19 are advertised by the reference board. **HW** for the ID set, **SRC** for the names.

| ID | Name | ID | Name |
|----|------|----|------|
| `0x01` | Colour wave (short) | `0x0B` | Blooming |
| `0x02` | Colour wave (long) | `0x0C` | Rainbow wave, vertical |
| `0x03` | Colour wheel | `0x0D` | Hurricane |
| `0x04` | Spectrum cycle | `0x0E` | Accumulate |
| `0x05` | Breathing | `0x0F` | Starlight (slow) |
| `0x06` | Static | `0x10` | Visor |
| `0x07` | Reactive (single key) | `0x11` | Surmount |
| `0x08` | Reactive ripple | `0x12` | Rainbow wave, circular |
| `0x09` | Reactive line | `0x14` | Custom (per-key map) |
| `0x0A` | Starlight (fast) | | |

`0x13` is not advertised. Which parameters each mode honours is in OpenRGB's mode flags and should be confirmed per mode. **SRC**

## 6. Warning: never write a canned capability block

dokutan changes the active profile by sending command `0x04` with a fixed 44-byte block captured from the author's own keyboard. That block contains device-specific bytes that differ on this board (§4). Sending it verbatim would overwrite them with another keyboard's values; the consequences are unknown. Any use of `0x04` must be read (`0x03`) → change payload byte 10 only → write back (AV-016, F-005).

## 7. Custom colour space (commands `0x10` / `0x11`)

- Flat array, 3 bytes per LED in R, G, B order. Address = `profile_index × 0x200 + led_index × 3`. **SRC**; single-key write at the right address confirmed on HW via OpenRGB (LED 59 = J).
- Bulk write: consecutive packets of up to 54 bytes. A full 118-slot map is 354 bytes, 7 packets. **SRC**
- A single key is one 3-byte packet inside a begin/end pair. **SRC**
- The reference board's custom map read back as all zeros (never set) on 2026-09-20. **HW**
- On 2026-10-04 the first packet of profile 1's map (LEDs 0–17) read back `ff 00 00` (red) for every LED, so the map has been written in between, presumably by OpenRGB during write-path validation. **HW** for the bytes; cause unconfirmed.
- Later the same day a full read of profile 1's map (118 slots, through `iris-device`) returned zero everywhere except slot 49 = `ff 00 00` and slot 65 = `ff ff ff`. LEDs 0–17 were no longer red. Neither slot is in the LED map (§8): 49 lies between `]` (47) and Caps Lock (52), 65 between `#` (64) and left Shift (69). **HW** for the bytes. The author had changed lighting with OpenRGB in between, which accounts for the rewrite. Which keys OpenRGB meant by slots 49 and 65 is not yet recorded. **OPEN**
- Two consecutive full reads, with nothing touching the keyboard in between, returned identical data, so `0x10` reads return stored colour memory, not live or scratch data. **HW**
- Whether `0x11` writes land in flash or RAM, and how long a transaction takes, is unmeasured. Iris assumes flash. **OPEN** (AV-004)

## 8. LED index map

Canonical data: `data/phantom_iso_uk_leds.toml`. Derived from dokutan's key-name → offset table (`offset ÷ 3`), numpad entries removed, leaving exactly 88 keys. **SRC**, with LED 59 = J confirmed. **HW**

Grid of 17 slots per row:

| Keys | LED indices |
|------|-------------|
| Esc, F1–F12 | 1–13 |
| `` ` ``, 1–0, `-`, `=` | 18–30; Backspace 98 |
| Tab, Q–P, `[`, `]` | 35–47 |
| Caps Lock, A–L, `;`, `'` | 52–63 |
| ISO `#` (beside Return) | 64: **OPEN**, expected from ANSI backslash position |
| Left Shift, Z–M, `,`, `.`, `/`, Right Shift | 69–80; Return 81 |
| ISO `\` (beside left Shift) | 105: **OPEN** |
| Ctrl, Super, Alt, Space, AltGr, Fn, Menu, Ctrl | 86–93 |
| Left, Down, Up, Right | 94–97 |
| PrtSc, ScrLk, Pause | 106–108 |
| Insert, Home, PgUp, Delete, End, PgDn | 110–115 |

## 9. Open questions and the experiments that settle them

1. **LEDs 64 and 105.** Light each in OpenRGB; record which key responds.
2. **Apply latency and flash-vs-RAM.** Time a single-key transaction and a full-map transaction. Then: write a key colour, power-cycle the keyboard, read back with `0x10`. Persisting proves non-volatile storage.
3. **Brightness and speed ranges.** Set each candidate value through `0x06`, read back with `0x05`, observe the keyboard.
4. **`0x04` read-modify-write.** Read the block, write it back byte-identical, read again and compare. Only then change byte 10.
5. **Checksum validation.** Deferred; requires sending a deliberately bad packet, which is not worth the risk until there is a reason.
6. **Post-incident keymap check.** Confirm every key and Fn combination behaves normally after BUG-001.
7. **Vendor-software captures.** usbmon + Wireshark against the Tecware software in a Windows VM with USB passthrough. Prerequisite for F-027/F-028 and the only route to discovering any undocumented direct mode.
