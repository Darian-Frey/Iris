//! The capability block returned by command `0x03` (PROTOCOL.md §4).

use crate::address::{Mode, Profile};
use crate::{Error, Result};

/// Size of the capability block. HW.
pub const CAPABILITY_SIZE: usize = 0x2C;

/// EVision V1 magic. V2 boards report `aa 55` instead (D-004). HW.
pub const CAPABILITY_MAGIC: [u8; 2] = [0x55, 0xaa];

const ACTIVE_PROFILE: usize = 10;
const MODE_LIST: usize = 16;

/// The 44-byte capability block.
///
/// Most of its bytes are of unknown meaning and differ between board
/// variants, so the block is kept whole and offers no constructor other than
/// parsing what the device sent (AV-016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    raw: [u8; CAPABILITY_SIZE],
}

impl Capabilities {
    /// Parses the payload of a `0x03` reply. Fails unless it begins with the
    /// V1 magic, which is part of the identity check before writes (AV-007).
    pub fn parse(payload: &[u8]) -> Result<Capabilities> {
        let raw: [u8; CAPABILITY_SIZE] = payload
            .get(..CAPABILITY_SIZE)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(Error::PayloadTooShort {
                len: payload.len(),
                need: CAPABILITY_SIZE,
            })?;
        if raw[..2] != CAPABILITY_MAGIC {
            return Err(Error::BadMagic([raw[0], raw[1]]));
        }
        Ok(Capabilities { raw })
    }

    /// The active onboard profile (payload byte 10). SRC; read as profile 1
    /// on hardware. `None` if the byte is not 0–2.
    pub fn active_profile(&self) -> Option<Profile> {
        Profile::from_index(self.raw[ACTIVE_PROFILE])
    }

    /// The modes the keyboard advertises, from payload byte 16 onwards.
    ///
    /// The list is read until the first byte that is not a known mode ID.
    /// What follows the 19 entries on the reference board is not recorded in
    /// PROTOCOL.md, so no terminator value is assumed.
    pub fn advertised_modes(&self) -> Vec<Mode> {
        self.raw[MODE_LIST..]
            .iter()
            .map_while(|&id| Mode::from_id(id))
            .collect()
    }

    /// The block exactly as read.
    pub fn raw(&self) -> &[u8; CAPABILITY_SIZE] {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 35 bytes PROTOCOL.md §4 records from the reference board. The
    /// remaining 9 bytes of the 44-byte payload were not recorded; they are
    /// zero here. Replace with the full capture when available.
    const REFERENCE: [u8; 35] = [
        0x55, 0xaa, 0xff, 0x02, 0x0f, 0x32, 0x64, 0x50, 0x02, 0x01, 0x00, 0x50, 0x00, 0x00, 0x00,
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
        0x0f, 0x11, 0x10, 0x12, 0x14,
    ];

    fn reference_payload() -> [u8; CAPABILITY_SIZE] {
        let mut payload = [0u8; CAPABILITY_SIZE];
        payload[..REFERENCE.len()].copy_from_slice(&REFERENCE);
        payload
    }

    #[test]
    fn parses_reference_board() {
        let caps = Capabilities::parse(&reference_payload()).unwrap();
        assert_eq!(caps.active_profile(), Some(Profile::One));
        let modes = caps.advertised_modes();
        assert_eq!(modes.len(), 19);
        let mut sorted: Vec<u8> = modes.iter().map(|m| m.id()).collect();
        sorted.sort_unstable();
        let mut all: Vec<u8> = Mode::ALL.iter().map(|m| m.id()).collect();
        all.sort_unstable();
        assert_eq!(sorted, all);
        assert_eq!(caps.raw()[..], reference_payload()[..]);
    }

    #[test]
    fn rejects_v2_magic_and_short_payloads() {
        let mut payload = reference_payload();
        payload[..2].copy_from_slice(&[0xaa, 0x55]);
        assert_eq!(
            Capabilities::parse(&payload),
            Err(Error::BadMagic([0xaa, 0x55]))
        );
        assert_eq!(
            Capabilities::parse(&REFERENCE),
            Err(Error::PayloadTooShort { len: 35, need: 44 })
        );
    }

    #[test]
    fn unknown_active_profile_is_none() {
        let mut payload = reference_payload();
        payload[ACTIVE_PROFILE] = 3;
        assert_eq!(
            Capabilities::parse(&payload).unwrap().active_profile(),
            None
        );
    }
}
