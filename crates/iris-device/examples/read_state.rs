//! Reads the keyboard's full lighting state through iris-device.
//!
//! Sends reads only: 0x03 once (identity check), 0x05 three times (one per
//! profile) and 0x10 seven times (profile 1's colour map). Nothing is
//! written. Development tooling for Phase 1; the shipped daemon is the only
//! program meant to open the device (D-003).

use std::process::ExitCode;

use iris_device::{Device, DeviceError, discover, permission_help};
use iris_proto::{COLOUR_SLOTS, Profile, Rgb};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(DeviceError::PermissionDenied { node }) => {
            eprintln!(
                "permission denied opening {}\n\n{}",
                node.display(),
                permission_help()
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), DeviceError> {
    let candidate = discover()?
        .into_iter()
        .next()
        .ok_or(DeviceError::NotFound)?;
    println!("node: {}", candidate.node.display());
    let mut device = Device::open(&candidate)?;

    println!("write access: {:?}", device.write_access());
    let capabilities = device.capabilities();
    println!("active profile: {:?}", capabilities.active_profile());
    println!("advertised modes: {:?}", capabilities.advertised_modes());
    println!("capability block: {}", hex(capabilities.raw()));

    for profile in Profile::ALL {
        let block = device.read_config(profile)?;
        println!(
            "\n{profile:?}: mode {:?} (id {:#04x}), brightness {}, speed {}, direction {}, \
             random {:?}, colour {:?}",
            block.mode(),
            block.mode_id(),
            block.brightness(),
            block.speed(),
            block.direction(),
            block.random_colour(),
            block.mode_colour(),
        );
        println!("  raw: {}", hex(block.raw()));
    }

    let map = device.read_colour_map(Profile::One)?;
    println!(
        "\nProfile 1 colour map ({} of {COLOUR_SLOTS} slots non-zero):",
        non_zero(&map)
    );
    for (led, colour) in map
        .iter()
        .enumerate()
        .filter(|(_, c)| **c != Rgb::default())
    {
        println!(
            "  LED {led:3}: {:02x} {:02x} {:02x}",
            colour.r, colour.g, colour.b
        );
    }
    println!("\nwrite packets sent: {}", device.write_packets());
    Ok(())
}

fn non_zero(map: &[Rgb]) -> usize {
    map.iter().filter(|c| **c != Rgb::default()).count()
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
