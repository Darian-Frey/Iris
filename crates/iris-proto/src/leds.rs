//! The LED index map (PROTOCOL.md §8, `data/phantom_iso_uk_leds.toml`).
//!
//! Key names are the public vocabulary above this crate (D-011); LED indices
//! do not leak upward.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::address::COLOUR_SLOTS;
use crate::{Error, Result};

const PHANTOM_ISO_UK: &str = include_str!("../../../data/phantom_iso_uk_leds.toml");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LedFile {
    leds: BTreeMap<String, String>,
}

/// A bidirectional map between key names and LED indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedMap {
    by_index: BTreeMap<usize, String>,
    by_name: BTreeMap<String, usize>,
}

impl LedMap {
    /// The built-in map for the 88-key ISO-UK Phantom.
    pub fn phantom_iso_uk() -> Result<LedMap> {
        LedMap::parse(PHANTOM_ISO_UK)
    }

    /// Parses an LED map file: a `[leds]` table of `index = "Name"`.
    pub fn parse(text: &str) -> Result<LedMap> {
        let file: LedFile = toml::from_str(text).map_err(|e| Error::LedMap(e.to_string()))?;
        let mut map = LedMap {
            by_index: BTreeMap::new(),
            by_name: BTreeMap::new(),
        };
        for (key, name) in file.leds {
            let index: usize = key
                .parse()
                .map_err(|_| Error::LedMap(format!("LED index {key:?} is not a number")))?;
            if index >= COLOUR_SLOTS {
                return Err(Error::LedMap(format!(
                    "LED index {index} is outside colour space"
                )));
            }
            if name.trim().is_empty() {
                return Err(Error::LedMap(format!("LED {index} has an empty name")));
            }
            if map.by_name.insert(name.clone(), index).is_some() {
                return Err(Error::LedMap(format!("key name {name:?} is used twice")));
            }
            // TOML rejects duplicate keys, but "7" and "07" are distinct keys.
            if map.by_index.insert(index, name).is_some() {
                return Err(Error::LedMap(format!("LED index {index} is used twice")));
            }
        }
        Ok(map)
    }

    /// The LED index of a key.
    pub fn index(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).copied()
    }

    /// The key at an LED index.
    pub fn name(&self, index: usize) -> Option<&str> {
        self.by_index.get(&index).map(String::as_str)
    }

    /// Number of keys.
    pub fn len(&self) -> usize {
        self.by_index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_index.is_empty()
    }

    /// Keys in LED index order.
    pub fn iter(&self) -> impl Iterator<Item = (usize, &str)> {
        self.by_index.iter().map(|(&i, name)| (i, name.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_map_has_88_keys() {
        let map = LedMap::phantom_iso_uk().unwrap();
        assert_eq!(map.len(), 88);
        // Confirmed on hardware.
        assert_eq!(map.index("J"), Some(59));
        assert_eq!(map.name(59), Some("J"));
        // Unverified ISO keys (AV-006), present at their expected indices.
        assert_eq!(map.index("Hash"), Some(64));
        assert_eq!(map.index("Backslash_ISO"), Some(105));
    }

    #[test]
    fn builtin_map_matches_protocol_grid() {
        let map = LedMap::phantom_iso_uk().unwrap();
        let expect = |names: &[&str], first: usize| {
            for (i, name) in names.iter().enumerate() {
                assert_eq!(map.index(name), Some(first + i), "{name}");
            }
        };
        expect(&["Esc", "F1"], 1);
        expect(&["F12"], 13);
        expect(&["Backtick", "1"], 18);
        expect(&["Equals"], 30);
        expect(&["Backspace"], 98);
        expect(&["Tab", "Q"], 35);
        expect(&["Caps_Lock", "A"], 52);
        expect(&["Shift_l", "Z"], 69);
        expect(&["Shift_r", "Return"], 80);
        expect(
            &[
                "Ctrl_l", "Super_l", "Alt_l", "Space", "Alt_r", "Fn", "Menu", "Ctrl_r",
            ],
            86,
        );
        expect(&["Left", "Down", "Up", "Right"], 94);
        expect(&["PrtSc", "ScrLk", "Pause"], 106);
        expect(&["Insert", "Home", "PgUp", "Delete", "End", "PgDn"], 110);
    }

    #[test]
    fn rejects_malformed_maps() {
        assert!(LedMap::parse("[leds]\nx = \"A\"").is_err());
        assert!(LedMap::parse("[leds]\n118 = \"A\"").is_err());
        assert!(LedMap::parse("[leds]\n1 = \"A\"\n2 = \"A\"").is_err());
        assert!(LedMap::parse("[leds]\n7 = \"A\"\n07 = \"B\"").is_err());
        assert!(LedMap::parse("[leds]\n1 = \" \"").is_err());
        assert!(LedMap::parse("[other]\n1 = \"A\"").is_err());
        assert!(LedMap::parse("not toml").is_err());
    }
}
