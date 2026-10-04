//! Replies captured from the reference keyboard, for tests.
//!
//! `fixtures/probe-2026-10-04.txt` is the verbatim output of
//! `tools/phantom_probe.py /dev/hidraw1`, run by the author on 2026-10-04. It
//! describes the keyboard's state at that moment; if the profiles are changed
//! later the capture stays valid as parser input but no longer describes the
//! keyboard (IMP-004).

use crate::PACKET_LEN;

const PROBE_2026_10_04: &str = include_str!("../fixtures/probe-2026-10-04.txt");

pub(crate) const CAPABILITIES: &str = "0x03 read profile / capabilities";
pub(crate) const CONFIG: [&str; 3] = [
    "0x05 read config, profile 1",
    "0x05 read config, profile 2",
    "0x05 read config, profile 3",
];
pub(crate) const COLOURS: &str = "0x10 read custom colours (V1)";

/// The 64-byte reply under the probe heading `== label ==`.
pub(crate) fn probe_reply(label: &str) -> [u8; PACKET_LEN] {
    let heading = format!("== {label} ==");
    let bytes: Vec<u8> = PROBE_2026_10_04
        .lines()
        .skip_while(|line| *line != heading)
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.trim().split_once(": "))
        .filter(|(offset, _)| u8::from_str_radix(offset, 16).is_ok())
        .flat_map(|(_, hex)| hex.split_whitespace())
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect();
    bytes
        .try_into()
        .unwrap_or_else(|b: Vec<u8>| panic!("{label}: {} bytes", b.len()))
}
