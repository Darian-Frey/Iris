//! Test access to the reference capture (see [`crate::capture`]).

pub(crate) use crate::capture::{CAPABILITIES, COLOURS, CONFIG};
use crate::{PACKET_LEN, capture};

/// The 64-byte reply under `== label ==` in the 2026-10-04 capture.
pub(crate) fn probe_reply(label: &str) -> [u8; PACKET_LEN] {
    capture::probe_reply(capture::REFERENCE_2026_10_04, label)
        .unwrap_or_else(|| panic!("capture has no 64-byte reply for {label:?}"))
}
