#!/usr/bin/env python3
"""
Read-only probe for the Tecware Phantom (EVision 320f:5064) vendor HID channel.

Sends ONLY read commands. Nothing is written to the keyboard's settings.
Usage:  sudo python3 phantom_probe.py [/dev/hidrawN]     (default: /dev/hidraw1)
"""
import os, select, sys, time

DEV = sys.argv[1] if len(sys.argv) > 1 else "/dev/hidraw1"
REPORT_ID = 0x04

# (label, command, size, offset)
PROBES = [
    ("0x03 read profile / capabilities", 0x03, 0x2C, 0x0000),
    ("0x05 read config, profile 1",      0x05, 0x38, 0x0000),
    ("0x05 read config, profile 2",      0x05, 0x38, 0x002A),
    ("0x05 read config, profile 3",      0x05, 0x38, 0x0054),
    ("0x10 read custom colours (V1)",    0x10, 0x36, 0x0000),
]


def hexdump(buf):
    for i in range(0, len(buf), 16):
        chunk = buf[i:i + 16]
        print(f"    {i:02x}: " + " ".join(f"{b:02x}" for b in chunk))


def build(cmd, size, offset):
    pkt = bytearray(64)
    pkt[0] = REPORT_ID
    pkt[3] = cmd
    pkt[4] = size
    pkt[5] = offset & 0xFF
    pkt[6] = (offset >> 8) & 0xFF
    csum = sum(pkt[3:]) & 0xFFFF
    pkt[1] = csum & 0xFF
    pkt[2] = csum >> 8
    return bytes(pkt)


def read_reply(fd, timeout=1.0):
    """Return the next report with our report ID, skipping anything else."""
    deadline = time.monotonic() + timeout
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return None
        ready, _, _ = select.select([fd], [], [], remaining)
        if not ready:
            return None
        data = os.read(fd, 64)
        if data and data[0] == REPORT_ID:
            return data
        print(f"    (skipped unrelated report, id 0x{data[0]:02x})")


def main():
    sysfs = f"/sys/class/hidraw/{os.path.basename(DEV)}/device/report_descriptor"
    try:
        with open(sysfs, "rb") as f:
            desc = f.read()
        print(f"Report descriptor for {DEV} ({len(desc)} bytes):")
        hexdump(desc)
    except OSError as e:
        print(f"Could not read report descriptor: {e}")

    try:
        fd = os.open(DEV, os.O_RDWR)
    except OSError as e:
        sys.exit(f"Cannot open {DEV}: {e}  (run with sudo?)")

    try:
        for label, cmd, size, offset in PROBES:
            print(f"\n== {label} ==")
            pkt = build(cmd, size, offset)
            try:
                os.write(fd, pkt)
            except OSError as e:
                print(f"    write failed: {e}")
                continue
            reply = read_reply(fd)
            if reply is None:
                print("    no reply (timeout)")
                continue
            echoed = reply[3] == cmd
            status = reply[7]
            print(f"    cmd echoed: {echoed}   status byte[7]: 0x{status:02x}"
                  f"   reply size byte[4]: 0x{reply[4]:02x}")
            hexdump(reply)
            time.sleep(0.05)
    finally:
        os.close(fd)


if __name__ == "__main__":
    main()
