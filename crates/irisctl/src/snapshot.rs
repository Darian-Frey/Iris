//! A record of the keyboard's lighting state, saved before every write.
//!
//! This is the minimal Phase 1 form of F-029: `first-before-write.txt` is
//! written once and never replaced, so the state before Iris ever wrote is
//! kept; `last-before-write.txt` is replaced on every write command. There is
//! no restore command yet; the files are for the author to read.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use iris_device::{Keyboard, Transport};
use iris_proto::{COLOUR_SLOTS, Profile};

fn snapshot_dir() -> Result<PathBuf> {
    let state = match std::env::var_os("XDG_STATE_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            PathBuf::from(std::env::var_os("HOME").context("HOME is not set")?).join(".local/state")
        }
    };
    Ok(state.join("iris/snapshots"))
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Renders the shadow copy as text.
pub fn render<T: Transport>(keyboard: &Keyboard<T>) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "# Iris lighting snapshot (irisctl, before a write)");
    let _ = writeln!(
        text,
        "capability: {}",
        hex(keyboard.device().capabilities().raw())
    );
    for profile in Profile::ALL {
        let number = profile.index() + 1;
        let _ = writeln!(
            text,
            "profile {number} config: {}",
            hex(keyboard.config(profile).raw())
        );
        let colours = keyboard.colours(profile);
        for start in (0..COLOUR_SLOTS).step_by(18) {
            let end = (start + 18).min(COLOUR_SLOTS);
            let bytes: Vec<u8> = colours[start..end]
                .iter()
                .flat_map(|c| [c.r, c.g, c.b])
                .collect();
            let _ = writeln!(
                text,
                "profile {number} colours {start:3}-{:3}: {}",
                end - 1,
                hex(&bytes)
            );
        }
    }
    text
}

/// Saves the snapshot; returns the path of the file written for this run.
pub fn save<T: Transport>(keyboard: &Keyboard<T>) -> Result<PathBuf> {
    let dir = snapshot_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let text = render(keyboard);
    let first = dir.join("first-before-write.txt");
    if !first.exists() {
        fs::write(&first, &text).with_context(|| format!("writing {}", first.display()))?;
    }
    let last = dir.join("last-before-write.txt");
    fs::write(&last, &text).with_context(|| format!("writing {}", last.display()))?;
    Ok(last)
}
