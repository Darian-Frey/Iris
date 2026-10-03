//! Addresses and value types for configuration and colour space
//! (PROTOCOL.md §5, §7).

use crate::{Error, Result};

/// Distance between consecutive profiles in configuration space. SRC stride,
/// HW-coherent reads at `0x00`, `0x2A`, `0x54`.
pub const CONFIG_STRIDE: u16 = 0x2A;

/// Distance between consecutive profiles in colour space. SRC.
pub const COLOUR_STRIDE: u16 = 0x200;

/// Number of 3-byte LED slots in one profile's colour map. SRC.
pub const COLOUR_SLOTS: usize = 118;

/// One of the keyboard's three onboard profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Profile {
    One,
    Two,
    Three,
}

impl Profile {
    /// All profiles, in order.
    pub const ALL: [Profile; 3] = [Profile::One, Profile::Two, Profile::Three];

    /// Zero-based index, as used on the wire.
    pub const fn index(self) -> u8 {
        match self {
            Profile::One => 0,
            Profile::Two => 1,
            Profile::Three => 2,
        }
    }

    /// Profile from a zero-based wire index.
    pub const fn from_index(index: u8) -> Option<Profile> {
        match index {
            0 => Some(Profile::One),
            1 => Some(Profile::Two),
            2 => Some(Profile::Three),
            _ => None,
        }
    }
}

/// A parameter in one profile's configuration block (PROTOCOL.md §5).
///
/// Only parameters PROTOCOL.md names are representable; there is no way to
/// address an undocumented byte of the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Parameter {
    /// HW (read).
    Mode,
    /// HW (read). Range OPEN.
    Brightness,
    /// HW (read). Range OPEN.
    Speed,
    /// SRC.
    Direction,
    /// HW (read). `0xFF` on, `0x00` off.
    RandomColour,
    /// HW (read). Three bytes, R G B.
    ModeColour,
    /// SRC.
    PollingRate,
    /// SRC. "Surmount" mode colour selector.
    SurmountColour,
}

impl Parameter {
    /// Offset of the parameter within a profile's block.
    pub const fn offset(self) -> u16 {
        match self {
            Parameter::Mode => 0x00,
            Parameter::Brightness => 0x01,
            Parameter::Speed => 0x02,
            Parameter::Direction => 0x03,
            Parameter::RandomColour => 0x04,
            Parameter::ModeColour => 0x05,
            Parameter::PollingRate => 0x0F,
            Parameter::SurmountColour => 0x11,
        }
    }

    /// Size of the parameter in bytes.
    pub const fn size(self) -> usize {
        match self {
            Parameter::ModeColour => 3,
            _ => 1,
        }
    }
}

/// Address of a parameter: `profile × 0x2A + parameter`.
pub const fn config_address(profile: Profile, parameter: Parameter) -> u16 {
    profile.index() as u16 * CONFIG_STRIDE + parameter.offset()
}

/// Address of an LED's colour: `profile × 0x200 + led × 3`.
pub fn colour_address(profile: Profile, led: usize) -> Result<u16> {
    if led >= COLOUR_SLOTS {
        return Err(Error::LedOutOfRange { led });
    }
    // led < 118, so led * 3 < 0x200 and the sum fits in u16.
    Ok(profile.index() as u16 * COLOUR_STRIDE + led as u16 * 3)
}

/// A built-in lighting mode (PROTOCOL.md §5 Modes). HW for the ID set, SRC
/// for the names. `0x13` is not advertised by the reference board and is not
/// representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Mode {
    ColourWaveShort = 0x01,
    ColourWaveLong = 0x02,
    ColourWheel = 0x03,
    SpectrumCycle = 0x04,
    Breathing = 0x05,
    Static = 0x06,
    ReactiveSingle = 0x07,
    ReactiveRipple = 0x08,
    ReactiveLine = 0x09,
    StarlightFast = 0x0A,
    Blooming = 0x0B,
    RainbowWaveVertical = 0x0C,
    Hurricane = 0x0D,
    Accumulate = 0x0E,
    StarlightSlow = 0x0F,
    Visor = 0x10,
    Surmount = 0x11,
    RainbowWaveCircular = 0x12,
    Custom = 0x14,
}

impl Mode {
    /// Every mode, in ID order.
    pub const ALL: [Mode; 19] = [
        Mode::ColourWaveShort,
        Mode::ColourWaveLong,
        Mode::ColourWheel,
        Mode::SpectrumCycle,
        Mode::Breathing,
        Mode::Static,
        Mode::ReactiveSingle,
        Mode::ReactiveRipple,
        Mode::ReactiveLine,
        Mode::StarlightFast,
        Mode::Blooming,
        Mode::RainbowWaveVertical,
        Mode::Hurricane,
        Mode::Accumulate,
        Mode::StarlightSlow,
        Mode::Visor,
        Mode::Surmount,
        Mode::RainbowWaveCircular,
        Mode::Custom,
    ];

    /// The mode ID on the wire.
    pub const fn id(self) -> u8 {
        self as u8
    }

    /// Mode from its wire ID.
    pub fn from_id(id: u8) -> Option<Mode> {
        Mode::ALL.into_iter().find(|mode| mode.id() == id)
    }
}

/// USB polling rate (parameter `0x0F`). SRC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PollingRate {
    Hz125 = 0,
    Hz250 = 1,
    Hz500 = 2,
    Hz1000 = 3,
}

impl PollingRate {
    /// Polling rate from its wire value.
    pub const fn from_byte(byte: u8) -> Option<PollingRate> {
        match byte {
            0 => Some(PollingRate::Hz125),
            1 => Some(PollingRate::Hz250),
            2 => Some(PollingRate::Hz500),
            3 => Some(PollingRate::Hz1000),
            _ => None,
        }
    }
}

/// A colour as sent to the device, in R, G, B byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_addresses_match_hardware_reads() {
        assert_eq!(config_address(Profile::One, Parameter::Mode), 0x00);
        assert_eq!(config_address(Profile::Two, Parameter::Mode), 0x2A);
        assert_eq!(config_address(Profile::Three, Parameter::Mode), 0x54);
        assert_eq!(config_address(Profile::Two, Parameter::ModeColour), 0x2F);
    }

    #[test]
    fn colour_addresses() {
        // LED 59 = J, confirmed on hardware via OpenRGB.
        assert_eq!(colour_address(Profile::One, 59), Ok(177));
        assert_eq!(colour_address(Profile::Three, 0), Ok(0x400));
        assert_eq!(
            colour_address(Profile::Three, COLOUR_SLOTS - 1),
            Ok(0x400 + 117 * 3)
        );
        assert_eq!(
            colour_address(Profile::One, COLOUR_SLOTS),
            Err(Error::LedOutOfRange { led: COLOUR_SLOTS })
        );
    }

    #[test]
    fn mode_ids_round_trip_and_skip_0x13() {
        for mode in Mode::ALL {
            assert_eq!(Mode::from_id(mode.id()), Some(mode));
        }
        assert_eq!(Mode::from_id(0x13), None);
        assert_eq!(Mode::from_id(0x00), None);
    }

    #[test]
    fn profile_indices_round_trip() {
        for profile in Profile::ALL {
            assert_eq!(Profile::from_index(profile.index()), Some(profile));
        }
        assert_eq!(Profile::from_index(3), None);
    }
}
