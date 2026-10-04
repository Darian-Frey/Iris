//! Command logic, generic over the transport so tests can run it against
//! the simulated keyboard.

use std::io::{BufRead, Write};

use anyhow::{Result, bail};
use iris_device::{Keyboard, Transport, WriteAccess};
use iris_proto::{COLOUR_SLOTS, LedMap, MAX_COLOUR_PAYLOAD, Mode, Profile, Rgb, Setting};

use crate::names::{describe, format_colour, mode_name, profile_number};

const COLOURS_PER_PACKET: usize = MAX_COLOUR_PAYLOAD / 3;

/// Everything a write command needs.
pub struct Writer<'a, T: Transport> {
    pub keyboard: &'a mut Keyboard<T>,
    pub out: &'a mut dyn Write,
    /// Show what would be sent, send nothing.
    pub dry_run: bool,
    /// Called once, after planning and before the first write packet.
    pub before_write: &'a mut dyn FnMut(&Keyboard<T>) -> Result<()>,
}

fn check_writable<T: Transport>(keyboard: &Keyboard<T>) -> Result<()> {
    if let WriteAccess::ReadOnly(why) = keyboard.device().write_access() {
        bail!("writes refused: {why}");
    }
    Ok(())
}

/// Warns when colours written to `profile` will not be visible.
fn visibility_notes<T: Transport>(
    keyboard: &Keyboard<T>,
    profile: Profile,
    out: &mut dyn Write,
) -> Result<()> {
    let number = profile_number(profile);
    if keyboard.config(profile).mode() != Some(Mode::Custom) {
        writeln!(
            out,
            "Note: profile {number} is not in custom mode, so these colours will not show \
             until it is (`irisctl set-mode --profile {number} custom`)."
        )?;
    }
    if keyboard.device().capabilities().active_profile() != Some(profile) {
        writeln!(
            out,
            "Note: profile {number} is not the active profile; switch to it with the \
             keyboard's Fn keys to see it."
        )?;
    }
    Ok(())
}

/// Applies settings to one profile.
pub fn set_mode<T: Transport>(
    w: &mut Writer<'_, T>,
    profile: Profile,
    settings: &[Setting],
) -> Result<()> {
    let number = profile_number(profile);
    let changed = w.keyboard.changed_settings(profile, settings);
    if changed.is_empty() {
        writeln!(
            w.out,
            "Profile {number} already has these settings; nothing sent."
        )?;
        return Ok(());
    }
    writeln!(
        w.out,
        "Profile {number}: {} write packet(s), one per changed setting: {changed:?}",
        changed.len()
    )?;
    if w.dry_run {
        writeln!(w.out, "Dry run: nothing sent.")?;
        return Ok(());
    }
    check_writable(w.keyboard)?;
    (w.before_write)(w.keyboard)?;
    let applied = w.keyboard.apply_settings(profile, settings)?;
    writeln!(
        w.out,
        "Sent {} write packet(s); read-back confirmed.\nProfile {number}: {}",
        applied.packets,
        describe(w.keyboard.config(profile))
    )?;
    Ok(())
}

/// Sets LED colours in one profile.
pub fn set_colours<T: Transport>(
    w: &mut Writer<'_, T>,
    profile: Profile,
    changes: &[(usize, Rgb)],
) -> Result<()> {
    let number = profile_number(profile);
    let runs = w.keyboard.changed_runs(profile, changes)?;
    if runs.is_empty() {
        writeln!(
            w.out,
            "Profile {number} already has these colours; nothing sent."
        )?;
        return Ok(());
    }
    let leds: usize = runs.iter().map(|(start, end)| end - start).sum();
    let packets: usize = runs
        .iter()
        .map(|(start, end)| (end - start).div_ceil(COLOURS_PER_PACKET))
        .sum();
    writeln!(
        w.out,
        "Profile {number}: {leds} LED(s) differ, in {} run(s): {packets} write packet(s).",
        runs.len()
    )?;
    if w.dry_run {
        writeln!(w.out, "Dry run: nothing sent.")?;
        return Ok(());
    }
    check_writable(w.keyboard)?;
    (w.before_write)(w.keyboard)?;
    let applied = w.keyboard.apply_colours(profile, changes)?;
    writeln!(
        w.out,
        "Sent {} write packet(s); read-back confirmed.",
        applied.packets
    )?;
    visibility_notes(w.keyboard, profile, w.out)?;
    Ok(())
}

/// What the user saw during one step of a walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    /// The key the LED map names lit up.
    AsExpected,
    /// Nothing visibly lit.
    Nothing,
    /// The user typed something other than the map's name for the key:
    /// either a different key, or the same key described by its legend.
    Other(String),
    /// The walk stopped before this LED.
    NotTried,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkStep {
    pub led: usize,
    pub expected: Option<String>,
    pub seen: Seen,
}

/// Lights one LED at a time and asks which key lit (AV-006, PROTOCOL.md §9
/// item 1). The profile must be the active one. Its mode is switched to
/// custom if needed, every slot is blanked, and afterwards the original
/// colours and mode are restored, also when the walk stops early or fails.
pub fn walk<T: Transport>(
    w: &mut Writer<'_, T>,
    profile: Profile,
    leds: &[usize],
    colour: Rgb,
    map: &LedMap,
    input: &mut dyn BufRead,
) -> Result<Vec<WalkStep>> {
    let number = profile_number(profile);
    if w.dry_run {
        bail!("walk writes on every step; it has no dry run");
    }
    if let Some(&led) = leds.iter().find(|&&led| led >= COLOUR_SLOTS) {
        bail!("LED {led} is outside colour space (0-{})", COLOUR_SLOTS - 1);
    }
    check_writable(w.keyboard)?;
    let active = w.keyboard.device().capabilities().active_profile();
    if active != Some(profile) {
        bail!(
            "profile {number} is not the active profile ({active:?}); switch to it with \
             the keyboard's Fn keys first (Iris cannot switch profiles yet, F-005)"
        );
    }
    (w.before_write)(w.keyboard)?;

    let original_mode = w.keyboard.config(profile).mode_id();
    let original_colours: Vec<(usize, Rgb)> = w
        .keyboard
        .colours(profile)
        .iter()
        .copied()
        .enumerate()
        .collect();

    let mut steps: Vec<WalkStep> = leds
        .iter()
        .map(|&led| WalkStep {
            led,
            expected: map.name(led).map(str::to_string),
            seen: Seen::NotTried,
        })
        .collect();
    let walked = walk_steps(w, profile, colour, input, &mut steps);

    writeln!(w.out, "Restoring profile {number}'s colours and mode...")?;
    let restored = restore(w.keyboard, profile, original_mode, &original_colours);
    walked?;
    restored?;
    writeln!(
        w.out,
        "Restored. {} write packet(s) sent this session.",
        w.keyboard.device().write_packets()
    )?;
    Ok(steps)
}

fn walk_steps<T: Transport>(
    w: &mut Writer<'_, T>,
    profile: Profile,
    colour: Rgb,
    input: &mut dyn BufRead,
    steps: &mut [WalkStep],
) -> Result<()> {
    w.keyboard
        .apply_settings(profile, &[Setting::Mode(Mode::Custom)])?;
    let black: Vec<(usize, Rgb)> = (0..COLOUR_SLOTS).map(|led| (led, Rgb::default())).collect();
    w.keyboard.apply_colours(profile, &black)?;

    let total = steps.len();
    let mut previous = None;
    for (i, step) in steps.iter_mut().enumerate() {
        let mut changes = vec![(step.led, colour)];
        if let Some(led) = previous {
            changes.insert(0, (led, Rgb::default()));
        }
        w.keyboard.apply_colours(profile, &changes)?;
        previous = Some(step.led);

        let expected = step.expected.as_deref().unwrap_or("no key in the LED map");
        write!(
            w.out,
            "[{}/{total}] LED {} lit (map says: {expected}). Enter = that key, \
             none = nothing lit, quit = stop, or type the key you see: ",
            i + 1,
            step.led
        )?;
        w.out.flush()?;
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            break;
        }
        step.seen = match line.trim() {
            // Control words are whole words, because single letters such as
            // `n` and `q` are also key legends.
            "" => Seen::AsExpected,
            typed if typed.eq_ignore_ascii_case("none") => Seen::Nothing,
            typed if typed.eq_ignore_ascii_case("quit") => break,
            typed
                if step
                    .expected
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case(typed)) =>
            {
                Seen::AsExpected
            }
            typed => Seen::Other(typed.to_string()),
        };
    }
    Ok(())
}

fn restore<T: Transport>(
    keyboard: &mut Keyboard<T>,
    profile: Profile,
    original_mode: u8,
    original_colours: &[(usize, Rgb)],
) -> Result<()> {
    keyboard.apply_colours(profile, original_colours)?;
    match Mode::from_id(original_mode) {
        Some(mode) => {
            keyboard.apply_settings(profile, &[Setting::Mode(mode)])?;
        }
        None => bail!(
            "original mode {original_mode:#04x} is not a known mode and was not restored; \
             profile {} is left in custom mode",
            profile_number(profile)
        ),
    }
    Ok(())
}

/// Prints walk results as a table.
pub fn report_walk(steps: &[WalkStep], out: &mut dyn Write) -> Result<()> {
    writeln!(out, "\nLED  map says        seen")?;
    for step in steps {
        let expected = step.expected.as_deref().unwrap_or("-");
        let seen = match &step.seen {
            Seen::AsExpected => "as expected".to_string(),
            Seen::Nothing => "nothing lit".to_string(),
            Seen::Other(typed) => format!("typed: {typed}"),
            Seen::NotTried => "not tried".to_string(),
        };
        writeln!(out, "{:3}  {expected:16}  {seen}", step.led)?;
    }
    Ok(())
}

/// Prints one profile's settings and per-key colours.
pub fn show_profile<T: Transport>(
    keyboard: &Keyboard<T>,
    profile: Profile,
    map: &LedMap,
    out: &mut dyn Write,
) -> Result<()> {
    let number = profile_number(profile);
    writeln!(
        out,
        "Profile {number}: {}",
        describe(keyboard.config(profile))
    )?;
    let colours = keyboard.colours(profile);
    for (led, name) in map.iter() {
        writeln!(out, "  {name:14} {}", format_colour(colours[led]))?;
    }
    let others: Vec<String> = colours
        .iter()
        .enumerate()
        .filter(|&(led, colour)| map.name(led).is_none() && *colour != Rgb::default())
        .map(|(led, colour)| format!("{led} = {}", format_colour(*colour)))
        .collect();
    if !others.is_empty() {
        writeln!(
            out,
            "  Slots outside the LED map holding a colour: {}",
            others.join(", ")
        )?;
    }
    Ok(())
}

/// Names the mode list, for `irisctl modes`.
pub fn list_modes(out: &mut dyn Write) -> Result<()> {
    for mode in Mode::ALL {
        writeln!(out, "{:#04x}  {}", mode.id(), mode_name(mode))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iris_device::sim::SimulatedKeyboard;
    use iris_device::{Device, PRODUCT_ID, REFERENCE_BCD_DEVICE, UsbIdentity, VENDOR_ID};
    use std::time::Duration;

    fn keyboard(bcd: u16) -> Keyboard<SimulatedKeyboard> {
        let usb = UsbIdentity {
            vendor_id: VENDOR_ID,
            product_id: PRODUCT_ID,
            bcd_device: Some(bcd),
        };
        let mut device = Device::connect(SimulatedKeyboard::reference(), usb).unwrap();
        device.set_timeout(Duration::from_millis(50));
        Keyboard::load(device).unwrap()
    }

    /// Runs `f` with a writer; returns its output and how often the
    /// pre-write hook (the snapshot) ran.
    fn run<R>(
        keyboard: &mut Keyboard<SimulatedKeyboard>,
        dry_run: bool,
        f: impl FnOnce(&mut Writer<'_, SimulatedKeyboard>) -> R,
    ) -> (R, String, usize) {
        let mut out = Vec::new();
        let mut hooks = 0;
        let mut hook = |_: &Keyboard<SimulatedKeyboard>| {
            hooks += 1;
            Ok(())
        };
        let mut writer = Writer {
            keyboard,
            out: &mut out,
            dry_run,
            before_write: &mut hook,
        };
        let result = f(&mut writer);
        (result, String::from_utf8(out).unwrap(), hooks)
    }

    fn traffic(keyboard: &Keyboard<SimulatedKeyboard>) -> usize {
        keyboard.device().transport().received().len()
    }

    #[test]
    fn dry_run_sends_nothing() {
        let mut kb = keyboard(REFERENCE_BCD_DEVICE);
        let before = traffic(&kb);
        let (result, out, hooks) = run(&mut kb, true, |w| {
            set_mode(w, Profile::One, &[Setting::Mode(Mode::Static)])?;
            set_colours(w, Profile::One, &[(59, Rgb::new(0, 255, 0))])
        });
        result.unwrap();
        assert!(out.contains("Dry run"));
        assert_eq!(hooks, 0);
        assert_eq!(traffic(&kb), before);
    }

    #[test]
    fn set_mode_writes_once_then_is_free() {
        let mut kb = keyboard(REFERENCE_BCD_DEVICE);
        let settings = [Setting::Mode(Mode::Static), Setting::Brightness(4)];
        let (result, out, hooks) = run(&mut kb, false, |w| set_mode(w, Profile::Two, &settings));
        result.unwrap();
        assert!(out.contains("Sent 1 write packet"), "{out}");
        assert_eq!(hooks, 1);
        let before = traffic(&kb);
        let (result, out, hooks) = run(&mut kb, false, |w| set_mode(w, Profile::Two, &settings));
        result.unwrap();
        assert!(out.contains("nothing sent"));
        assert_eq!(hooks, 0);
        assert_eq!(traffic(&kb), before);
    }

    #[test]
    fn set_colours_warns_when_not_visible() {
        let mut kb = keyboard(REFERENCE_BCD_DEVICE);
        let (result, out, _) = run(&mut kb, false, |w| {
            set_colours(w, Profile::Two, &[(59, Rgb::new(0, 0, 255))])
        });
        result.unwrap();
        assert!(out.contains("not in custom mode"));
        assert!(out.contains("not the active profile"));
        assert_eq!(kb.colours(Profile::Two)[59], Rgb::new(0, 0, 255));
    }

    #[test]
    fn read_only_device_refuses_before_the_snapshot() {
        let mut kb = keyboard(0x0103);
        let (result, _, hooks) = run(&mut kb, false, |w| {
            set_mode(w, Profile::One, &[Setting::Speed(1)])
        });
        assert!(result.unwrap_err().to_string().contains("writes refused"));
        assert_eq!(hooks, 0);
        assert_eq!(kb.device().transport().write_packets(), 0);
    }

    #[test]
    fn walk_lights_each_led_and_restores_everything() {
        let mut kb = keyboard(REFERENCE_BCD_DEVICE);
        let map = LedMap::phantom_iso_uk().unwrap();
        let original = kb.colours(Profile::One).to_vec();
        let mut input = "\nnone\nAltGr\n".as_bytes();
        // (Enter, nothing lit, a different key.)
        let (result, _, hooks) = run(&mut kb, false, |w| {
            walk(
                w,
                Profile::One,
                &[64, 105, 59],
                Rgb::new(255, 255, 255),
                &map,
                &mut input,
            )
        });
        let steps = result.unwrap();
        assert_eq!(hooks, 1);
        assert_eq!(steps[0].seen, Seen::AsExpected);
        assert_eq!(steps[0].expected.as_deref(), Some("Hash"));
        assert_eq!(steps[1].seen, Seen::Nothing);
        assert_eq!(steps[2].seen, Seen::Other("AltGr".into()));
        assert_eq!(kb.colours(Profile::One), &original[..]);
        assert_eq!(kb.config(Profile::One).mode(), Some(Mode::SpectrumCycle));
        let sim = kb.device().transport();
        assert_eq!(sim.config(Profile::One)[0], Mode::SpectrumCycle.id());
        assert!(sim.violations().is_empty());
    }

    #[test]
    fn typing_the_expected_name_counts_as_a_match() {
        let mut kb = keyboard(REFERENCE_BCD_DEVICE);
        let map = LedMap::phantom_iso_uk().unwrap();
        let mut input = "j\n\\\n".as_bytes();
        let (result, _, _) = run(&mut kb, false, |w| {
            walk(
                w,
                Profile::One,
                &[59, 105],
                Rgb::new(255, 255, 255),
                &map,
                &mut input,
            )
        });
        let steps = result.unwrap();
        assert_eq!(steps[0].seen, Seen::AsExpected);
        assert_eq!(steps[1].seen, Seen::Other("\\".into()));
    }

    #[test]
    fn single_letter_legends_are_answers_not_commands() {
        // LED 75 is N and LED 36 is Q: typing their legends must not be read
        // as "nothing lit" or "stop".
        let mut kb = keyboard(REFERENCE_BCD_DEVICE);
        let map = LedMap::phantom_iso_uk().unwrap();
        let mut input = "n\nq\nNONE\n".as_bytes();
        let (result, _, _) = run(&mut kb, false, |w| {
            walk(
                w,
                Profile::One,
                &[75, 36, 64],
                Rgb::new(255, 255, 255),
                &map,
                &mut input,
            )
        });
        let steps = result.unwrap();
        assert_eq!(steps[0].seen, Seen::AsExpected);
        assert_eq!(steps[1].seen, Seen::AsExpected);
        assert_eq!(steps[2].seen, Seen::Nothing);
    }

    #[test]
    fn walk_stopped_early_still_restores() {
        let mut kb = keyboard(REFERENCE_BCD_DEVICE);
        let map = LedMap::phantom_iso_uk().unwrap();
        let original = kb.colours(Profile::One).to_vec();
        let mut input = "quit\n".as_bytes();
        let (result, _, _) = run(&mut kb, false, |w| {
            walk(
                w,
                Profile::One,
                &[64, 105],
                Rgb::new(255, 255, 255),
                &map,
                &mut input,
            )
        });
        let steps = result.unwrap();
        assert!(steps.iter().all(|s| s.seen == Seen::NotTried));
        assert_eq!(kb.colours(Profile::One), &original[..]);
        assert_eq!(kb.config(Profile::One).mode(), Some(Mode::SpectrumCycle));
    }

    #[test]
    fn walk_refuses_an_inactive_profile_without_writing() {
        let mut kb = keyboard(REFERENCE_BCD_DEVICE);
        let map = LedMap::phantom_iso_uk().unwrap();
        let mut input = "".as_bytes();
        let (result, _, hooks) = run(&mut kb, false, |w| {
            walk(
                w,
                Profile::Two,
                &[64],
                Rgb::new(255, 255, 255),
                &map,
                &mut input,
            )
        });
        assert!(result.is_err());
        assert_eq!(hooks, 0);
        assert_eq!(kb.device().transport().write_packets(), 0);
    }
}
