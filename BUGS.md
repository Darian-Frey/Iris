# Bugs

Catalogue of bugs discovered during development. Per the project workflow,
bugs are **logged here when found, not silently fixed** (see Maintenance
Rule 8). The author decides whether to fix immediately, defer, or leave
alone.

Status vocabulary: open | fixed | wontfix | deferred.
Severity vocabulary: low | medium | high.

## Open

None.

## Fixed

### BUG-001: Reconnaissance probe sent a V1 keymap-header write, believing it to be a V2 read
**Status:** fixed (2026-09-20, same session)
**Found:** 2026-09-20 (while decoding the probe's output against dokutan's keymap packets)
**Location:** [tools/phantom_probe.py](tools/phantom_probe.py) `PROBES` table
**Severity:** high (unverified write to keymap storage on irreplaceable firmware)
**Description.** To find out whether the firmware spoke the EVision V2 dialect, the probe sent command `0x0A` with size `0x38` and offset 0, which is "read custom colours" in V2. On V1 firmware `0x0A` is the keymap header write (payload normally begins `aa 55`). The command list had been taken from OpenRGB's V2 controller without being checked against V1's write commands. The keyboard replied with status `0x00` and 56 zero bytes, which says nothing about whether the write was applied. The packet was sent without the begin/end wrapper that normally commits settings, so it may have been ignored; this is unconfirmed.
**Reproduction.** Run the first version of the probe. Do not.
**Notes.** Fix: the `0x0A` entry was removed; the script now sends only `0x03`, `0x05`, `0x10`. Hardware follow-up is still outstanding: confirm every key and Fn combination behaves normally (PROTOCOL.md §9 item 6, ROADMAP Phase 0 carried items). If anything is wrong, use the keyboard's factory-reset key combination from the Tecware manual. Root cause addressed structurally by D-004 and D-005; failure mode recorded as AV-001 and AV-002. The same incident motivates the gate in D-015 and the proposal in IMP-001.

## Won't Fix

None.

## Deferred

None.
