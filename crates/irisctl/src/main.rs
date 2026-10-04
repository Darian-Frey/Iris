//! `irisctl`, Phase 1 direct mode.
//!
//! Until `irisd` exists (Phase 2), this talks to the keyboard itself through
//! `iris-device`, under the development-tool exception in CLAUDE.md. Every
//! write goes through the allow-list, the identity check, the transaction
//! guard and the shadow-state diff; a snapshot of the lighting state is saved
//! before each write command.

mod commands;
mod names;
mod snapshot;

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use iris_device::{Device, DeviceError, Hidraw, Keyboard, discover, permission_help};
use iris_proto::{COLOUR_SLOTS, LedMap, Profile, Rgb, Setting};
use serde::Deserialize;

use commands::Writer;
use names::{key_led, parse_colour, parse_mode, profile, profile_number};

#[derive(Parser)]
#[command(
    name = "irisctl",
    version,
    about = "Control the Tecware Phantom RGB (Phase 1: talks to the keyboard directly)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the keyboard, its identity and each profile's settings (reads only).
    Info,
    /// Show settings and per-key colours (reads only).
    Get {
        /// Profile 1-3; all three if omitted.
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
        profile: Option<u8>,
    },
    /// List key names (no device access).
    Keys,
    /// List modes (no device access).
    Modes,
    /// Set a profile's mode and parameters.
    SetMode {
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
        profile: u8,
        /// Mode name; see `irisctl modes`.
        mode: String,
        /// Brightness. The valid range is not yet known (OpenRGB uses 0-4,
        /// dokutan 0-9); values above 9 are refused.
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=9))]
        brightness: Option<u8>,
        /// Speed. The valid range is not yet known (OpenRGB uses 0-5,
        /// dokutan 0-3); values above 5 are refused.
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=5))]
        speed: Option<u8>,
        /// Mode colour, rrggbb.
        #[arg(long)]
        colour: Option<String>,
        /// Random colours: on or off.
        #[arg(long, value_parser = ["on", "off"])]
        random: Option<String>,
        /// Show what would be sent; send nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Set one key's custom colour.
    SetKey {
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
        profile: u8,
        /// Key name; see `irisctl keys`.
        key: String,
        /// Colour, rrggbb.
        colour: String,
        #[arg(long)]
        dry_run: bool,
    },
    /// Set many keys' custom colours from a TOML file and/or a fill colour.
    ///
    /// File format: optional `fill = "rrggbb"`, then a `[keys]` table of
    /// `Name = "rrggbb"`. Keys not mentioned keep their colour; slots outside
    /// the LED map are never touched.
    SetMap {
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
        profile: u8,
        /// Map file.
        file: Option<PathBuf>,
        /// Colour for every key in the LED map before the file is applied.
        #[arg(long)]
        fill: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Light one LED at a time and record which key lights (AV-006).
    ///
    /// Targets are key names or LED numbers written `#64`. The profile must be
    /// the active one; its colours and mode are restored afterwards.
    Walk {
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
        profile: u8,
        /// Keys (`Hash`) or LEDs (`#64`).
        targets: Vec<String>,
        /// Walk every key in the LED map.
        #[arg(long, conflicts_with_all = ["targets", "unmapped"])]
        all: bool,
        /// Walk every colour slot the LED map does not assign to a key.
        #[arg(long, conflicts_with = "targets")]
        unmapped: bool,
        /// EXPERIMENTAL: walk colour slots 118-169, beyond the LED map, whose
        /// meaning is unknown (PROTOCOL.md §9 item 8). Reads them first and
        /// writes every original byte back.
        #[arg(long, conflicts_with_all = ["targets", "all", "unmapped"])]
        beyond_map: bool,
        /// Colour to light each LED with.
        #[arg(long, default_value = "ffffff")]
        colour: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MapFile {
    fill: Option<String>,
    #[serde(default)]
    keys: BTreeMap<String, String>,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(DeviceError::PermissionDenied { node }) = error.downcast_ref() {
                eprintln!(
                    "Permission denied opening {}.\n\n{}",
                    node.display(),
                    permission_help()
                );
            } else {
                eprintln!("irisctl: {error:#}");
            }
            ExitCode::FAILURE
        }
    }
}

fn open() -> Result<Keyboard<Hidraw>> {
    let candidate = discover()?
        .into_iter()
        .next()
        .ok_or(DeviceError::NotFound)?;
    let device = Device::open(&candidate)?;
    Ok(Keyboard::load(device)?)
}

fn save_snapshot(keyboard: &Keyboard<Hidraw>) -> Result<()> {
    let path = snapshot::save(keyboard)?;
    println!("Saved lighting snapshot to {}", path.display());
    Ok(())
}

fn profiles(number: Option<u8>) -> Result<Vec<Profile>> {
    Ok(match number {
        Some(n) => vec![profile(n)?],
        None => Profile::ALL.to_vec(),
    })
}

fn run(cli: Cli) -> Result<()> {
    let map = LedMap::phantom_iso_uk()?;
    let mut stdout = io::stdout();
    match cli.command {
        Command::Keys => {
            for (led, name) in map.iter() {
                writeln!(stdout, "{led:3}  {name}")?;
            }
            Ok(())
        }
        Command::Modes => commands::list_modes(&mut stdout),
        Command::Info => {
            let keyboard = open()?;
            let device = keyboard.device();
            let usb = device.usb();
            let firmware = usb.bcd_device.map_or("unknown".into(), |b| {
                format!("{:x}.{:02x}", b >> 8, b & 0xFF)
            });
            writeln!(
                stdout,
                "Keyboard {:04x}:{:04x}, firmware {firmware}",
                usb.vendor_id, usb.product_id
            )?;
            writeln!(stdout, "Write access: {:?}", device.write_access())?;
            let capabilities = device.capabilities();
            let active = capabilities
                .active_profile()
                .map_or("unknown".into(), |p| profile_number(p).to_string());
            writeln!(stdout, "Active profile: {active}")?;
            writeln!(
                stdout,
                "Advertised modes: {}",
                capabilities.advertised_modes().len()
            )?;
            for p in Profile::ALL {
                writeln!(
                    stdout,
                    "Profile {}: {}",
                    profile_number(p),
                    names::describe(keyboard.config(p))
                )?;
            }
            Ok(())
        }
        Command::Get { profile } => {
            let keyboard = open()?;
            for p in profiles(profile)? {
                commands::show_profile(&keyboard, p, &map, &mut stdout)?;
            }
            Ok(())
        }
        Command::SetMode {
            profile: number,
            mode,
            brightness,
            speed,
            colour,
            random,
            dry_run,
        } => {
            let mut settings = vec![Setting::Mode(parse_mode(&mode)?)];
            settings.extend(brightness.map(Setting::Brightness));
            settings.extend(speed.map(Setting::Speed));
            if let Some(colour) = colour {
                settings.push(Setting::ModeColour(parse_colour(&colour)?));
            }
            if let Some(random) = random {
                settings.push(Setting::RandomColour(random == "on"));
            }
            let mut keyboard = open()?;
            let mut hook = save_snapshot;
            let mut writer = Writer {
                keyboard: &mut keyboard,
                out: &mut stdout,
                dry_run,
                before_write: &mut hook,
            };
            commands::set_mode(&mut writer, profile(number)?, &settings)
        }
        Command::SetKey {
            profile: number,
            key,
            colour,
            dry_run,
        } => {
            let change = (key_led(&map, &key)?, parse_colour(&colour)?);
            let mut keyboard = open()?;
            let mut hook = save_snapshot;
            let mut writer = Writer {
                keyboard: &mut keyboard,
                out: &mut stdout,
                dry_run,
                before_write: &mut hook,
            };
            commands::set_colours(&mut writer, profile(number)?, &[change])
        }
        Command::SetMap {
            profile: number,
            file,
            fill,
            dry_run,
        } => {
            let changes = map_changes(&map, file, fill)?;
            let mut keyboard = open()?;
            let mut hook = save_snapshot;
            let mut writer = Writer {
                keyboard: &mut keyboard,
                out: &mut stdout,
                dry_run,
                before_write: &mut hook,
            };
            commands::set_colours(&mut writer, profile(number)?, &changes)
        }
        Command::Walk {
            profile: number,
            targets,
            all,
            unmapped,
            beyond_map,
            colour,
        } => {
            if beyond_map {
                let colour = parse_colour(&colour)?;
                let mut keyboard = open()?;
                let mut hook = save_snapshot;
                let mut stdin = io::stdin().lock();
                let mut writer = Writer {
                    keyboard: &mut keyboard,
                    out: &mut stdout,
                    dry_run: false,
                    before_write: &mut hook,
                };
                let steps =
                    commands::walk_beyond_map(&mut writer, profile(number)?, colour, &mut stdin)?;
                return commands::report_walk(&steps, &mut stdout);
            }
            let leds: Vec<usize> = if all {
                map.iter().map(|(led, _)| led).collect()
            } else if unmapped {
                (0..COLOUR_SLOTS)
                    .filter(|&led| map.name(led).is_none())
                    .collect()
            } else if targets.is_empty() {
                bail!(
                    "name keys or LEDs to walk (for example `Hash #105`), or pass --all, \
                     --unmapped or --beyond-map"
                );
            } else {
                targets
                    .iter()
                    .map(|target| walk_target(&map, target))
                    .collect::<Result<_>>()?
            };
            let colour = parse_colour(&colour)?;
            let mut keyboard = open()?;
            let mut hook = save_snapshot;
            let mut stdin = io::stdin().lock();
            let mut writer = Writer {
                keyboard: &mut keyboard,
                out: &mut stdout,
                dry_run: false,
                before_write: &mut hook,
            };
            let steps = commands::walk(
                &mut writer,
                profile(number)?,
                &leds,
                colour,
                &map,
                &mut stdin,
            )?;
            commands::report_walk(&steps, &mut stdout)
        }
    }
}

fn walk_target(map: &LedMap, target: &str) -> Result<usize> {
    match target.strip_prefix('#') {
        Some(number) => number
            .parse()
            .map_err(|_| anyhow!("{target:?} is not an LED number such as #64")),
        None => key_led(map, target),
    }
}

/// The colour changes a `set-map` invocation asks for.
fn map_changes(
    map: &LedMap,
    file: Option<PathBuf>,
    fill: Option<String>,
) -> Result<Vec<(usize, Rgb)>> {
    let parsed = match &file {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            Some(
                toml::from_str::<MapFile>(&text)
                    .with_context(|| format!("in {}", path.display()))?,
            )
        }
        None => None,
    };
    let fill = match (fill, parsed.as_ref().and_then(|f| f.fill.clone())) {
        (Some(fill), _) | (None, Some(fill)) => Some(parse_colour(&fill)?),
        (None, None) => None,
    };
    let mut changes: Vec<(usize, Rgb)> = Vec::new();
    if let Some(colour) = fill {
        changes.extend(map.iter().map(|(led, _)| (led, colour)));
    }
    if let Some(parsed) = parsed {
        for (key, colour) in &parsed.keys {
            changes.push((key_led(map, key)?, parse_colour(colour)?));
        }
    }
    if changes.is_empty() {
        bail!("nothing to set: give a map file, --fill, or both");
    }
    Ok(changes)
}
