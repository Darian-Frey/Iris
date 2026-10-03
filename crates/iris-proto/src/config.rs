//! Per-profile configuration blocks (PROTOCOL.md §5).

use crate::address::{CONFIG_STRIDE, Mode, Parameter, PollingRate, Rgb};
use crate::{Error, Result};

const BLOCK_LEN: usize = CONFIG_STRIDE as usize;

/// A value for one parameter, as written by command `0x06`.
///
/// Brightness, speed and direction are passed through unchecked: their valid
/// ranges are OPEN and the two prior implementations disagree (PROTOCOL.md §9
/// item 3). Range policy belongs to the caller until hardware settles it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Mode(Mode),
    Brightness(u8),
    Speed(u8),
    Direction(u8),
    RandomColour(bool),
    ModeColour(Rgb),
    PollingRate(PollingRate),
    SurmountColour(u8),
}

impl Setting {
    /// The parameter this setting writes.
    pub const fn parameter(self) -> Parameter {
        match self {
            Setting::Mode(_) => Parameter::Mode,
            Setting::Brightness(_) => Parameter::Brightness,
            Setting::Speed(_) => Parameter::Speed,
            Setting::Direction(_) => Parameter::Direction,
            Setting::RandomColour(_) => Parameter::RandomColour,
            Setting::ModeColour(_) => Parameter::ModeColour,
            Setting::PollingRate(_) => Parameter::PollingRate,
            Setting::SurmountColour(_) => Parameter::SurmountColour,
        }
    }

    /// Wire bytes, padded to three, and how many of them are meaningful.
    pub(crate) fn encode(self) -> ([u8; 3], usize) {
        let one = |byte: u8| ([byte, 0, 0], 1);
        match self {
            Setting::Mode(mode) => one(mode.id()),
            Setting::Brightness(value)
            | Setting::Speed(value)
            | Setting::Direction(value)
            | Setting::SurmountColour(value) => one(value),
            Setting::RandomColour(on) => one(if on { 0xFF } else { 0x00 }),
            Setting::ModeColour(rgb) => ([rgb.r, rgb.g, rgb.b], 3),
            Setting::PollingRate(rate) => one(rate as u8),
        }
    }
}

/// One profile's 42-byte configuration block, as read by command `0x05`.
///
/// The raw bytes are kept whole: most of the block's meaning is undocumented,
/// and nothing here may synthesise it (AV-016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigBlock {
    raw: [u8; BLOCK_LEN],
}

impl ConfigBlock {
    /// Parses the payload of a configuration read addressed at a profile's
    /// base. Bytes beyond the first 42 belong to the next profile and are
    /// ignored.
    pub fn parse(payload: &[u8]) -> Result<ConfigBlock> {
        let raw = payload
            .get(..BLOCK_LEN)
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(Error::PayloadTooShort {
                len: payload.len(),
                need: BLOCK_LEN,
            })?;
        Ok(ConfigBlock { raw })
    }

    fn byte(&self, parameter: Parameter) -> u8 {
        self.raw[usize::from(parameter.offset())]
    }

    /// The raw mode byte, which may name a mode Iris does not know.
    pub fn mode_id(&self) -> u8 {
        self.byte(Parameter::Mode)
    }

    pub fn mode(&self) -> Option<Mode> {
        Mode::from_id(self.mode_id())
    }

    pub fn brightness(&self) -> u8 {
        self.byte(Parameter::Brightness)
    }

    pub fn speed(&self) -> u8 {
        self.byte(Parameter::Speed)
    }

    pub fn direction(&self) -> u8 {
        self.byte(Parameter::Direction)
    }

    /// `Some(true)` for `0xFF`, `Some(false)` for `0x00`, `None` otherwise.
    pub fn random_colour(&self) -> Option<bool> {
        match self.byte(Parameter::RandomColour) {
            0xFF => Some(true),
            0x00 => Some(false),
            _ => None,
        }
    }

    pub fn mode_colour(&self) -> Rgb {
        let at = usize::from(Parameter::ModeColour.offset());
        Rgb::new(self.raw[at], self.raw[at + 1], self.raw[at + 2])
    }

    pub fn polling_rate(&self) -> Option<PollingRate> {
        PollingRate::from_byte(self.byte(Parameter::PollingRate))
    }

    pub fn surmount_colour(&self) -> u8 {
        self.byte(Parameter::SurmountColour)
    }

    /// The block exactly as read.
    pub fn raw(&self) -> &[u8; BLOCK_LEN] {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reconstructed from the values PROTOCOL.md §5 records for 2026-09-20.
    // The raw probe output was not preserved, so every byte not listed there
    // is zero here. Replace with the captured bytes when available.
    fn profile_one() -> [u8; 0x38] {
        let mut payload = [0u8; 0x38];
        payload[..8].copy_from_slice(&[0x04, 1, 2, 0, 0, 0x96, 0x96, 0x9a]);
        payload
    }

    fn profile_two_or_three() -> [u8; 0x38] {
        let mut payload = [0u8; 0x38];
        payload[..5].copy_from_slice(&[0x01, 4, 2, 0, 0xFF]);
        payload
    }

    #[test]
    fn parses_profile_one() {
        let block = ConfigBlock::parse(&profile_one()).unwrap();
        assert_eq!(block.mode(), Some(Mode::SpectrumCycle));
        assert_eq!(block.brightness(), 1);
        assert_eq!(block.speed(), 2);
        assert_eq!(block.mode_colour(), Rgb::new(0x96, 0x96, 0x9a));
    }

    #[test]
    fn parses_profiles_two_and_three() {
        let block = ConfigBlock::parse(&profile_two_or_three()).unwrap();
        assert_eq!(block.mode(), Some(Mode::ColourWaveShort));
        assert_eq!(block.brightness(), 4);
        assert_eq!(block.speed(), 2);
        assert_eq!(block.random_colour(), Some(true));
    }

    #[test]
    fn keeps_raw_bytes_and_ignores_next_profile() {
        let mut payload = profile_one();
        payload[0x2A] = 0xEE; // first byte of the next profile
        let block = ConfigBlock::parse(&payload).unwrap();
        assert_eq!(block.raw()[..], payload[..0x2A]);
    }

    #[test]
    fn rejects_short_payload_and_tolerates_unknown_values() {
        assert_eq!(
            ConfigBlock::parse(&[0u8; 41]),
            Err(Error::PayloadTooShort { len: 41, need: 42 })
        );
        let mut payload = [0u8; 42];
        payload[0x00] = 0x13;
        payload[0x04] = 0x7F;
        payload[0x0F] = 9;
        let block = ConfigBlock::parse(&payload).unwrap();
        assert_eq!((block.mode(), block.mode_id()), (None, 0x13));
        assert_eq!(block.random_colour(), None);
        assert_eq!(block.polling_rate(), None);
    }

    #[test]
    fn setting_encodings() {
        assert_eq!(Setting::Mode(Mode::Custom).encode(), ([0x14, 0, 0], 1));
        assert_eq!(Setting::RandomColour(true).encode(), ([0xFF, 0, 0], 1));
        assert_eq!(Setting::RandomColour(false).encode(), ([0x00, 0, 0], 1));
        assert_eq!(
            Setting::PollingRate(PollingRate::Hz1000).encode(),
            ([3, 0, 0], 1)
        );
        assert_eq!(
            Setting::ModeColour(Rgb::new(1, 2, 3)).encode(),
            ([1, 2, 3], 3)
        );
        for setting in [Setting::Brightness(0), Setting::ModeColour(Rgb::default())] {
            assert_eq!(setting.encode().1, setting.parameter().size());
        }
    }
}
