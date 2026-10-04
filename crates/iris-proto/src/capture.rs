//! Reading replies out of `tools/phantom_probe.py` output.
//!
//! The probe prints each reply as a hex dump under a `== label ==` heading.
//! This module turns those dumps back into 64-byte reports, so captures from
//! the real keyboard can serve as test fixtures and as the simulated
//! keyboard's starting state (IMP-004).

use crate::PACKET_LEN;

/// Verbatim probe output from the reference keyboard, run by the author on
/// 2026-10-04 against `/dev/hidraw1`. It describes the keyboard's state at
/// that moment; the colour map beyond the first 18 LEDs was not read.
pub const REFERENCE_2026_10_04: &str = include_str!("../fixtures/probe-2026-10-04.txt");

/// Probe heading of the capability read.
pub const CAPABILITIES: &str = "0x03 read profile / capabilities";

/// Probe headings of the three configuration reads, in profile order.
pub const CONFIG: [&str; 3] = [
    "0x05 read config, profile 1",
    "0x05 read config, profile 2",
    "0x05 read config, profile 3",
];

/// Probe heading of the colour read (profile 1, LEDs 0–17).
pub const COLOURS: &str = "0x10 read custom colours (V1)";

/// The 64-byte report printed under `== label ==` in probe output, or `None`
/// if the heading is missing or its dump is not exactly 64 valid hex bytes.
pub fn probe_reply(text: &str, label: &str) -> Option<[u8; PACKET_LEN]> {
    let heading = format!("== {label} ==");
    let mut bytes = Vec::with_capacity(PACKET_LEN);
    let dump = text
        .lines()
        .skip_while(|line| line.trim() != heading)
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.trim().split_once(": "))
        .filter(|(offset, _)| u8::from_str_radix(offset, 16).is_ok());
    for (_, hex) in dump {
        for byte in hex.split_whitespace() {
            bytes.push(u8::from_str_radix(byte, 16).ok()?);
        }
    }
    bytes.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_capture_has_every_reply() {
        for label in [CAPABILITIES, COLOURS].into_iter().chain(CONFIG) {
            let reply = probe_reply(REFERENCE_2026_10_04, label).unwrap();
            assert_eq!(reply[0], crate::REPORT_ID, "{label}");
        }
    }

    #[test]
    fn malformed_dumps_yield_none() {
        assert_eq!(probe_reply(REFERENCE_2026_10_04, "no such heading"), None);
        assert_eq!(probe_reply("== x ==\n    00: 04 zz\n", "x"), None);
        assert_eq!(probe_reply("== x ==\n    00: 04 2f\n", "x"), None);
    }
}
