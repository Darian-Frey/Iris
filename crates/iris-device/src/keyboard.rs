//! Shadow state and write minimisation (F-012, device side; AV-004).
//!
//! [`Keyboard`] reads the whole lighting state once, keeps a copy, and turns
//! each requested change into the smallest write that achieves it. A change
//! that matches the copy sends nothing at all, not even begin and end. After
//! every write it reads the affected area back, so the copy always reflects
//! what the keyboard reported rather than what was asked for.
//!
//! The rate limiter and the persistent write counter belong to `irisd`
//! (Phase 2); the in-session counters are on [`Device`].

use iris_proto::{COLOUR_SLOTS, ConfigBlock, Error as ProtoError, Profile, Rgb, Setting};

use crate::{Device, DeviceError, Transport};

/// What an apply call sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Applied {
    /// Data-write packets sent. Zero means the keyboard already matched and
    /// no transaction was opened.
    pub packets: usize,
}

/// A connected keyboard with a shadow copy of its lighting state.
#[derive(Debug)]
pub struct Keyboard<T: Transport> {
    device: Device<T>,
    config: Vec<ConfigBlock>,
    colours: Vec<Vec<Rgb>>,
    /// Set while a write is in flight, and left set if it fails, so the next
    /// call re-reads instead of diffing against a copy that may be wrong.
    stale: bool,
}

impl<T: Transport> Keyboard<T> {
    /// Reads every profile's configuration block and colour map (25 reads).
    pub fn load(device: Device<T>) -> Result<Keyboard<T>, DeviceError> {
        let mut keyboard = Keyboard {
            device,
            config: Vec::new(),
            colours: Vec::new(),
            stale: true,
        };
        keyboard.refresh()?;
        Ok(keyboard)
    }

    /// Re-reads the whole lighting state, replacing the shadow copy.
    pub fn refresh(&mut self) -> Result<(), DeviceError> {
        self.stale = true;
        let mut config = Vec::with_capacity(Profile::ALL.len());
        let mut colours = Vec::with_capacity(Profile::ALL.len());
        for profile in Profile::ALL {
            config.push(self.device.read_config(profile)?);
            colours.push(self.device.read_colour_map(profile)?);
        }
        self.config = config;
        self.colours = colours;
        self.stale = false;
        Ok(())
    }

    pub fn device(&self) -> &Device<T> {
        &self.device
    }

    /// The device: for fault injection in tests, and for the experimental
    /// slot probe, which addresses colour slots outside the shadow copy.
    pub fn device_mut(&mut self) -> &mut Device<T> {
        &mut self.device
    }

    /// The shadow copy of one profile's configuration block.
    pub fn config(&self, profile: Profile) -> &ConfigBlock {
        &self.config[usize::from(profile.index())]
    }

    /// The shadow copy of one profile's colour map (118 slots).
    pub fn colours(&self, profile: Profile) -> &[Rgb] {
        &self.colours[usize::from(profile.index())]
    }

    /// The settings the keyboard does not already hold, according to the
    /// shadow copy. This is exactly what [`apply_settings`](Self::apply_settings)
    /// would write.
    pub fn changed_settings(&self, profile: Profile, settings: &[Setting]) -> Vec<Setting> {
        let block = self.config(profile);
        settings
            .iter()
            .copied()
            .filter(|&setting| !block.holds(setting))
            .collect()
    }

    /// The runs of LEDs (half-open ranges) that differ from the shadow copy
    /// after `changes`. This is exactly what
    /// [`apply_colours`](Self::apply_colours) would write.
    pub fn changed_runs(
        &self,
        profile: Profile,
        changes: &[(usize, Rgb)],
    ) -> Result<Vec<(usize, usize)>, DeviceError> {
        Ok(differing_runs(
            self.colours(profile),
            &self.desired(profile, changes)?,
        ))
    }

    fn desired(&self, profile: Profile, changes: &[(usize, Rgb)]) -> Result<Vec<Rgb>, DeviceError> {
        let mut desired = self.colours(profile).to_vec();
        for &(led, colour) in changes {
            let slot = desired
                .get_mut(led)
                .ok_or(DeviceError::Protocol(ProtoError::LedOutOfRange { led }))?;
            *slot = colour;
        }
        Ok(desired)
    }

    /// Applies settings to one profile, writing only those the keyboard does
    /// not already hold, then reads the block back to confirm them.
    pub fn apply_settings(
        &mut self,
        profile: Profile,
        settings: &[Setting],
    ) -> Result<Applied, DeviceError> {
        self.ensure_fresh()?;
        let index = usize::from(profile.index());
        let changed = self.changed_settings(profile, settings);
        if changed.is_empty() {
            return Ok(Applied::default());
        }

        self.stale = true;
        let mut transaction = self.device.transaction()?;
        for &setting in &changed {
            transaction.set(profile, setting)?;
        }
        transaction.commit()?;

        let block = self.device.read_config(profile)?;
        let missing: Vec<Setting> = settings
            .iter()
            .copied()
            .filter(|&setting| !block.holds(setting))
            .collect();
        self.config[index] = block;
        self.stale = false;
        if !missing.is_empty() {
            return Err(DeviceError::ReadBackMismatch(format!(
                "{profile:?} does not hold {missing:?}"
            )));
        }
        Ok(Applied {
            packets: changed.len(),
        })
    }

    /// Sets the given LED colours in one profile, writing only the runs of
    /// LEDs that differ from the shadow copy, then reads the map back.
    pub fn apply_colours(
        &mut self,
        profile: Profile,
        changes: &[(usize, Rgb)],
    ) -> Result<Applied, DeviceError> {
        // Validate before any traffic, including a refresh.
        if let Some(&(led, _)) = changes.iter().find(|(led, _)| *led >= COLOUR_SLOTS) {
            return Err(DeviceError::Protocol(ProtoError::LedOutOfRange { led }));
        }
        self.ensure_fresh()?;
        let index = usize::from(profile.index());
        let desired = self.desired(profile, changes)?;
        let runs = differing_runs(&self.colours[index], &desired);
        if runs.is_empty() {
            return Ok(Applied::default());
        }

        self.stale = true;
        let before = self.device.write_packets();
        let mut transaction = self.device.transaction()?;
        for &(start, end) in &runs {
            transaction.write_colours(profile, start, &desired[start..end])?;
        }
        transaction.commit()?;
        let packets = usize::try_from(self.device.write_packets() - before).unwrap_or(usize::MAX);

        let read_back = self.device.read_colour_map(profile)?;
        let mismatched: Vec<usize> = (0..COLOUR_SLOTS)
            .filter(|&led| read_back[led] != desired[led])
            .collect();
        self.colours[index] = read_back;
        self.stale = false;
        if !mismatched.is_empty() {
            return Err(DeviceError::ReadBackMismatch(format!(
                "{profile:?} LEDs {mismatched:?} do not hold the written colours"
            )));
        }
        Ok(Applied { packets })
    }

    fn ensure_fresh(&mut self) -> Result<(), DeviceError> {
        if self.stale {
            self.refresh()?;
        }
        Ok(())
    }
}

/// Half-open index ranges where `desired` differs from `current`.
fn differing_runs(current: &[Rgb], desired: &[Rgb]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start = None;
    for (i, (a, b)) in current.iter().zip(desired).enumerate() {
        match (a != b, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                runs.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push((s, current.len()));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::SimulatedKeyboard;
    use crate::{PRODUCT_ID, REFERENCE_BCD_DEVICE, UsbIdentity, VENDOR_ID};
    use iris_proto::Mode;
    use std::time::Duration;

    fn load() -> Keyboard<SimulatedKeyboard> {
        let usb = UsbIdentity {
            vendor_id: VENDOR_ID,
            product_id: PRODUCT_ID,
            bcd_device: Some(REFERENCE_BCD_DEVICE),
        };
        let mut device = Device::connect(SimulatedKeyboard::reference(), usb).unwrap();
        device.set_timeout(Duration::from_millis(50));
        Keyboard::load(device).unwrap()
    }

    fn traffic(keyboard: &Keyboard<SimulatedKeyboard>) -> usize {
        keyboard.device().transport().received().len()
    }

    #[test]
    fn load_reads_everything_and_writes_nothing() {
        let keyboard = load();
        // 1 identity read + 3 × (1 config + 7 colour) reads.
        assert_eq!(traffic(&keyboard), 25);
        assert_eq!(keyboard.device().transport().write_packets(), 0);
        assert_eq!(
            keyboard.config(Profile::One).mode(),
            Some(Mode::SpectrumCycle)
        );
        assert_eq!(keyboard.colours(Profile::One)[0], Rgb::new(255, 0, 0));
    }

    #[test]
    fn unchanged_settings_send_nothing() {
        let mut keyboard = load();
        let before = traffic(&keyboard);
        let same = [Setting::Mode(Mode::SpectrumCycle), Setting::Brightness(4)];
        assert_eq!(
            keyboard
                .apply_settings(Profile::One, &same)
                .unwrap()
                .packets,
            0
        );
        assert_eq!(traffic(&keyboard), before);
    }

    #[test]
    fn only_changed_settings_are_written_and_reapplying_is_free() {
        let mut keyboard = load();
        let settings = [Setting::Brightness(4), Setting::Speed(3)];
        assert_eq!(
            keyboard
                .apply_settings(Profile::One, &settings)
                .unwrap()
                .packets,
            1
        );
        assert_eq!(keyboard.config(Profile::One).speed(), 3);
        let before = traffic(&keyboard);
        assert_eq!(
            keyboard
                .apply_settings(Profile::One, &settings)
                .unwrap()
                .packets,
            0
        );
        assert_eq!(traffic(&keyboard), before);
        assert!(keyboard.device().transport().violations().is_empty());
    }

    #[test]
    fn same_map_twice_sends_zero_packets_the_second_time() {
        // ROADMAP Phase 1 acceptance, against the simulator.
        let mut keyboard = load();
        let map: Vec<(usize, Rgb)> = (0..COLOUR_SLOTS)
            .map(|led| (led, Rgb::new(led as u8, 0, 255 - led as u8)))
            .collect();
        let first = keyboard.apply_colours(Profile::Two, &map).unwrap();
        assert_eq!(first.packets, 7);
        let before = traffic(&keyboard);
        assert_eq!(
            keyboard.apply_colours(Profile::Two, &map).unwrap().packets,
            0
        );
        assert_eq!(traffic(&keyboard), before);
        assert!(keyboard.device().transport().violations().is_empty());
    }

    #[test]
    fn single_key_change_is_one_packet() {
        let mut keyboard = load();
        let applied = keyboard
            .apply_colours(Profile::One, &[(59, Rgb::new(0, 255, 0))])
            .unwrap();
        assert_eq!(applied.packets, 1);
        assert_eq!(keyboard.colours(Profile::One)[59], Rgb::new(0, 255, 0));
        assert_eq!(
            keyboard.device().transport().colour(Profile::One, 59),
            Some(Rgb::new(0, 255, 0))
        );
    }

    #[test]
    fn separate_runs_are_written_separately() {
        let mut keyboard = load();
        let white = Rgb::new(255, 255, 255);
        let applied = keyboard
            .apply_colours(Profile::Three, &[(1, white), (2, white), (59, white)])
            .unwrap();
        assert_eq!(applied.packets, 2);
        // Re-setting LED 0 to the red it already holds costs nothing.
        let applied = keyboard
            .apply_colours(Profile::One, &[(0, Rgb::new(255, 0, 0))])
            .unwrap();
        assert_eq!(applied.packets, 0);
    }

    #[test]
    fn out_of_range_led_is_rejected_before_any_traffic() {
        let mut keyboard = load();
        let before = traffic(&keyboard);
        assert!(matches!(
            keyboard.apply_colours(Profile::One, &[(COLOUR_SLOTS, Rgb::default())]),
            Err(DeviceError::Protocol(ProtoError::LedOutOfRange { .. }))
        ));
        assert_eq!(traffic(&keyboard), before);
    }

    #[test]
    fn unconfirmed_write_is_reported_and_the_copy_follows_the_keyboard() {
        let mut keyboard = load();
        keyboard.device_mut().transport_mut().discard_writes(true);
        assert!(matches!(
            keyboard.apply_settings(Profile::One, &[Setting::Speed(5)]),
            Err(DeviceError::ReadBackMismatch(_))
        ));
        assert_eq!(keyboard.config(Profile::One).speed(), 0);
        assert!(matches!(
            keyboard.apply_colours(Profile::One, &[(59, Rgb::new(1, 2, 3))]),
            Err(DeviceError::ReadBackMismatch(_))
        ));
        assert_eq!(keyboard.colours(Profile::One)[59], Rgb::default());
    }

    #[test]
    fn failed_write_forces_a_refresh_before_the_next_diff() {
        let mut keyboard = load();
        keyboard.device_mut().transport_mut().drop_replies(2);
        assert!(
            keyboard
                .apply_settings(Profile::One, &[Setting::Speed(5)])
                .is_err()
        );
        let before = traffic(&keyboard);
        keyboard
            .apply_settings(Profile::One, &[Setting::Speed(5)])
            .unwrap();
        // The second call re-read all 24 blocks before diffing.
        assert!(traffic(&keyboard) - before >= 24);
        assert_eq!(keyboard.config(Profile::One).speed(), 5);
    }

    #[test]
    fn runs_are_found_correctly() {
        let a = vec![Rgb::default(); 6];
        let mut b = a.clone();
        assert!(differing_runs(&a, &b).is_empty());
        b[0] = Rgb::new(1, 0, 0);
        b[2] = Rgb::new(1, 0, 0);
        b[3] = Rgb::new(1, 0, 0);
        b[5] = Rgb::new(1, 0, 0);
        assert_eq!(differing_runs(&a, &b), [(0, 1), (2, 4), (5, 6)]);
    }
}
