//! Names users type: modes, colours, keys.

use anyhow::{Context, Result, anyhow, bail};
use iris_proto::{ConfigBlock, LedMap, Mode, Profile, Rgb};

/// Command-line names for the modes, in ID order (PROTOCOL.md §5; the names
/// are SRC).
pub const MODE_NAMES: [(Mode, &str); 19] = [
    (Mode::ColourWaveShort, "colour-wave-short"),
    (Mode::ColourWaveLong, "colour-wave-long"),
    (Mode::ColourWheel, "colour-wheel"),
    (Mode::SpectrumCycle, "spectrum-cycle"),
    (Mode::Breathing, "breathing"),
    (Mode::Static, "static"),
    (Mode::ReactiveSingle, "reactive-single"),
    (Mode::ReactiveRipple, "reactive-ripple"),
    (Mode::ReactiveLine, "reactive-line"),
    (Mode::StarlightFast, "starlight-fast"),
    (Mode::Blooming, "blooming"),
    (Mode::RainbowWaveVertical, "rainbow-wave-vertical"),
    (Mode::Hurricane, "hurricane"),
    (Mode::Accumulate, "accumulate"),
    (Mode::StarlightSlow, "starlight-slow"),
    (Mode::Visor, "visor"),
    (Mode::Surmount, "surmount"),
    (Mode::RainbowWaveCircular, "rainbow-wave-circular"),
    (Mode::Custom, "custom"),
];

pub fn mode_name(mode: Mode) -> &'static str {
    MODE_NAMES
        .iter()
        .find(|(m, _)| *m == mode)
        .map_or("unknown", |(_, name)| name)
}

pub fn parse_mode(text: &str) -> Result<Mode> {
    MODE_NAMES
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(text))
        .map(|(mode, _)| *mode)
        .ok_or_else(|| anyhow!("unknown mode {text:?}; `irisctl modes` lists them"))
}

pub fn profile(number: u8) -> Result<Profile> {
    number
        .checked_sub(1)
        .and_then(Profile::from_index)
        .ok_or_else(|| anyhow!("profile must be 1, 2 or 3"))
}

pub fn profile_number(profile: Profile) -> u8 {
    profile.index() + 1
}

/// Parses `rrggbb` or `#rrggbb`.
pub fn parse_colour(text: &str) -> Result<Rgb> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("colour {text:?} is not of the form rrggbb");
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).context("hex colour");
    Ok(Rgb::new(byte(0)?, byte(2)?, byte(4)?))
}

pub fn format_colour(colour: Rgb) -> String {
    format!("{:02x}{:02x}{:02x}", colour.r, colour.g, colour.b)
}

/// Finds a key by name, exactly or ignoring case.
pub fn key_led(map: &LedMap, name: &str) -> Result<usize> {
    map.index(name)
        .or_else(|| {
            map.iter()
                .find(|(_, key)| key.eq_ignore_ascii_case(name))
                .map(|(led, _)| led)
        })
        .ok_or_else(|| anyhow!("unknown key {name:?}; `irisctl keys` lists them"))
}

/// One line describing a profile's settings.
pub fn describe(block: &ConfigBlock) -> String {
    let mode = match block.mode() {
        Some(mode) => format!("{} ({:#04x})", mode_name(mode), mode.id()),
        None => format!("unknown ({:#04x})", block.mode_id()),
    };
    let random = match block.random_colour() {
        Some(true) => "on".to_string(),
        Some(false) => "off".to_string(),
        None => format!("unknown ({:#04x})", block.raw()[0x04]),
    };
    format!(
        "mode {mode}, brightness {}, speed {}, direction {}, random {random}, colour {}",
        block.brightness(),
        block.speed(),
        block.direction(),
        format_colour(block.mode_colour()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_has_a_unique_name() {
        for mode in Mode::ALL {
            let name = mode_name(mode);
            assert_ne!(name, "unknown");
            assert_eq!(parse_mode(name).unwrap(), mode);
        }
        assert_eq!(parse_mode("STATIC").unwrap(), Mode::Static);
        assert!(parse_mode("direct").is_err());
    }

    #[test]
    fn colours() {
        assert_eq!(parse_colour("ff8000").unwrap(), Rgb::new(255, 128, 0));
        assert_eq!(parse_colour("#00FF00").unwrap(), Rgb::new(0, 255, 0));
        for bad in ["fff", "gg0000", "ff00001", ""] {
            assert!(parse_colour(bad).is_err(), "{bad}");
        }
        assert_eq!(format_colour(Rgb::new(1, 2, 255)), "0102ff");
    }

    #[test]
    fn profiles_are_one_based() {
        assert_eq!(profile(1).unwrap(), Profile::One);
        assert_eq!(profile(3).unwrap(), Profile::Three);
        assert!(profile(0).is_err());
        assert!(profile(4).is_err());
    }

    #[test]
    fn keys_match_ignoring_case() {
        let map = LedMap::phantom_iso_uk().unwrap();
        assert_eq!(key_led(&map, "J").unwrap(), 59);
        assert_eq!(key_led(&map, "j").unwrap(), 59);
        assert_eq!(key_led(&map, "caps_lock").unwrap(), 52);
        assert!(key_led(&map, "Numpad5").is_err());
    }
}
