//! Request/reply exchanges, the identity check and write transactions.

use std::time::{Duration, Instant};

use iris_proto::{
    COLOUR_SLOTS, Capabilities, ConfigBlock, Error as ProtoError, Profile, Reply, Request, Rgb,
    Setting,
};

use crate::discovery::{Candidate, PRODUCT_ID, UsbIdentity, VENDOR_ID};
use crate::{DeviceError, Hidraw, Transport};

/// Firmware revision of the reference keyboard (`bcdDevice 1.02`). HW.
pub const REFERENCE_BCD_DEVICE: u16 = 0x0102;

/// How long to wait for a reply by default. The probe used one second.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);

/// The outcome of the identity check (AV-007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteAccess {
    /// VID/PID, firmware revision and capability magic all match the
    /// reference keyboard. Writes are allowed.
    Verified,
    /// The device answered with the V1 magic but is not the verified
    /// combination; it may be read but not written.
    ReadOnly(String),
}

/// A connected keyboard. All device traffic is sequential and goes through
/// one exchange routine that matches replies to requests (AV-008).
#[derive(Debug)]
pub struct Device<T: Transport> {
    link: Link<T>,
    usb: UsbIdentity,
    capabilities: Capabilities,
    access: WriteAccess,
}

/// The sequential request/reply channel underneath a [`Device`].
#[derive(Debug)]
struct Link<T: Transport> {
    transport: T,
    timeout: Duration,
    disconnected: bool,
    write_packets: u64,
    transactions: u64,
}

impl Device<Hidraw> {
    /// Opens a discovered node and runs the identity check.
    pub fn open(candidate: &Candidate) -> Result<Device<Hidraw>, DeviceError> {
        Device::connect(Hidraw::open(&candidate.node)?, candidate.usb)
    }
}

impl<T: Transport> Device<T> {
    /// Runs the identity check over `transport`: reads the capability block
    /// (a read, safe on any V1 board) and compares what sysfs and the device
    /// report against the reference keyboard. Fails outright if the magic is
    /// not V1's `55 aa`.
    pub fn connect(transport: T, usb: UsbIdentity) -> Result<Device<T>, DeviceError> {
        let mut link = Link {
            transport,
            timeout: DEFAULT_TIMEOUT,
            disconnected: false,
            write_packets: 0,
            transactions: 0,
        };
        let capabilities = match link.read_capabilities() {
            Ok(capabilities) => capabilities,
            Err(DeviceError::Protocol(ProtoError::BadMagic(magic))) => {
                return Err(DeviceError::IdentityMismatch(format!(
                    "capability magic {:02x} {:02x} is not EVision V1",
                    magic[0], magic[1]
                )));
            }
            Err(error) => return Err(error),
        };
        Ok(Device {
            link,
            usb,
            capabilities,
            access: check_identity(&usb),
        })
    }

    /// Changes how long each exchange waits for its reply.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.link.timeout = timeout;
    }

    pub fn usb(&self) -> UsbIdentity {
        self.usb
    }

    /// The capability block read during the identity check.
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn write_access(&self) -> &WriteAccess {
        &self.access
    }

    /// Data-write packets (`0x06`, `0x11`) sent in this session.
    pub fn write_packets(&self) -> u64 {
        self.link.write_packets
    }

    /// Write transactions opened in this session.
    pub fn transactions(&self) -> u64 {
        self.link.transactions
    }

    /// The transport, for inspection in tests.
    pub fn transport(&self) -> &T {
        &self.link.transport
    }

    /// Reads the capability block afresh.
    pub fn read_capabilities(&mut self) -> Result<Capabilities, DeviceError> {
        self.link.read_capabilities()
    }

    /// Reads one profile's configuration block.
    pub fn read_config(&mut self, profile: Profile) -> Result<ConfigBlock, DeviceError> {
        let reply = self.link.exchange(&Request::read_config(profile))?;
        Ok(ConfigBlock::parse(reply.payload())?)
    }

    /// Reads one profile's whole custom colour map (118 slots).
    pub fn read_colour_map(&mut self, profile: Profile) -> Result<Vec<Rgb>, DeviceError> {
        let mut colours = Vec::with_capacity(COLOUR_SLOTS);
        for request in Request::read_colour_map(profile) {
            let reply = self.link.exchange(&request)?;
            colours.extend(
                reply
                    .payload()
                    .chunks_exact(3)
                    .map(|rgb| Rgb::new(rgb[0], rgb[1], rgb[2])),
            );
        }
        Ok(colours)
    }

    /// Opens a write transaction. Refused unless the identity check passed.
    ///
    /// Once the begin packet has been sent, the returned guard sends the end
    /// packet on every path out: [`Transaction::commit`], an error, an early
    /// return or a panic (AV-005).
    pub fn transaction(&mut self) -> Result<Transaction<'_, T>, DeviceError> {
        if let WriteAccess::ReadOnly(why) = &self.access {
            return Err(DeviceError::WritesNotPermitted(why.clone()));
        }
        let begin = Request::begin();
        self.link.send(&begin)?;
        self.link.transactions += 1;
        let transaction = Transaction {
            link: &mut self.link,
            open: true,
        };
        // If the begin reply is lost, the guard still sends end on drop.
        transaction.link.await_reply(&begin)?;
        Ok(transaction)
    }
}

impl<T: Transport> Link<T> {
    fn read_capabilities(&mut self) -> Result<Capabilities, DeviceError> {
        let reply = self.exchange(&Request::read_capabilities())?;
        Ok(Capabilities::parse(reply.payload())?)
    }

    fn exchange(&mut self, request: &Request) -> Result<Reply, DeviceError> {
        self.send(request)?;
        self.await_reply(request)
    }

    fn send(&mut self, request: &Request) -> Result<(), DeviceError> {
        if self.disconnected {
            return Err(DeviceError::Disconnected);
        }
        let result = self.transport.send(request).map_err(DeviceError::from);
        if result.is_ok() && request.command().is_write() && request.size() > 0 {
            self.write_packets += 1;
        }
        self.note(result)
    }

    /// Waits for the reply that answers `request`, skipping key presses and
    /// media keys (other report IDs) and stale vendor replies to earlier,
    /// timed-out requests (AV-008).
    fn await_reply(&mut self, request: &Request) -> Result<Reply, DeviceError> {
        let deadline = Instant::now() + self.timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let received = self.transport.receive(remaining).map_err(DeviceError::from);
            let Some(report) = self.note(received)? else {
                return Err(DeviceError::Timeout);
            };
            match Reply::parse(&report) {
                Ok(reply) if reply.answers(request) => return Ok(reply),
                // A foreign report, a stale reply or noise: keep waiting.
                Ok(_) | Err(_) => {}
            }
            if remaining.is_zero() {
                return Err(DeviceError::Timeout);
            }
        }
    }

    /// Remembers a disconnection so later calls fail fast (AV-014).
    fn note<V>(&mut self, result: Result<V, DeviceError>) -> Result<V, DeviceError> {
        if matches!(result, Err(DeviceError::Disconnected)) {
            self.disconnected = true;
        }
        result
    }
}

fn check_identity(usb: &UsbIdentity) -> WriteAccess {
    if (usb.vendor_id, usb.product_id) != (VENDOR_ID, PRODUCT_ID) {
        return WriteAccess::ReadOnly(format!(
            "USB ID {:04x}:{:04x} is not the reference keyboard",
            usb.vendor_id, usb.product_id
        ));
    }
    match usb.bcd_device {
        Some(REFERENCE_BCD_DEVICE) => WriteAccess::Verified,
        Some(bcd) => WriteAccess::ReadOnly(format!(
            "firmware {:x}.{:02x} is not the verified 1.02",
            bcd >> 8,
            bcd & 0xFF
        )),
        None => WriteAccess::ReadOnly("firmware revision unknown".into()),
    }
}

/// An open write transaction. Dropping it without [`commit`](Self::commit)
/// still sends the end packet (AV-005).
///
/// If a write fails part-way, the end packet is still sent and the keyboard
/// may hold part of the change; callers must re-read state before trusting
/// any shadow copy.
#[derive(Debug)]
pub struct Transaction<'a, T: Transport> {
    link: &'a mut Link<T>,
    open: bool,
}

impl<T: Transport> Transaction<'_, T> {
    /// Sets one parameter of one profile.
    pub fn set(&mut self, profile: Profile, setting: Setting) -> Result<(), DeviceError> {
        self.link
            .exchange(&Request::set_parameter(profile, setting))
            .map(drop)
    }

    /// Writes consecutive LED colours starting at `first_led`, in as many
    /// packets as needed.
    pub fn write_colours(
        &mut self,
        profile: Profile,
        first_led: usize,
        colours: &[Rgb],
    ) -> Result<(), DeviceError> {
        for request in Request::write_colour_run(profile, first_led, colours)? {
            self.link.exchange(&request)?;
        }
        Ok(())
    }

    /// Sends the end packet and closes the transaction.
    pub fn commit(mut self) -> Result<(), DeviceError> {
        self.end()
    }

    fn end(&mut self) -> Result<(), DeviceError> {
        if !self.open {
            return Ok(());
        }
        // Closed as soon as end is attempted, so drop never sends a second.
        self.open = false;
        self.link.exchange(&Request::end()).map(drop)
    }
}

impl<T: Transport> Drop for Transaction<'_, T> {
    fn drop(&mut self) {
        // Nothing useful can be done with a failure here; the device layer
        // has already recorded a disconnection if that is what happened.
        let _ = self.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{SimulatedKeyboard, Violation};
    use iris_proto::{Mode, Setting};

    fn reference_usb() -> UsbIdentity {
        UsbIdentity {
            vendor_id: VENDOR_ID,
            product_id: PRODUCT_ID,
            bcd_device: Some(REFERENCE_BCD_DEVICE),
        }
    }

    fn connect() -> Device<SimulatedKeyboard> {
        let mut device = Device::connect(SimulatedKeyboard::reference(), reference_usb()).unwrap();
        device.set_timeout(Duration::from_millis(50));
        device
    }

    #[test]
    fn reference_keyboard_is_verified() {
        let device = connect();
        assert_eq!(device.write_access(), &WriteAccess::Verified);
        assert_eq!(device.capabilities().advertised_modes().len(), 19);
        assert_eq!(device.capabilities().active_profile(), Some(Profile::One));
    }

    #[test]
    fn unknown_firmware_is_read_only() {
        let usb = UsbIdentity {
            bcd_device: Some(0x0103),
            ..reference_usb()
        };
        let mut device = Device::connect(SimulatedKeyboard::reference(), usb).unwrap();
        assert!(matches!(device.write_access(), WriteAccess::ReadOnly(_)));
        assert!(device.read_config(Profile::One).is_ok());
        assert!(matches!(
            device.transaction(),
            Err(DeviceError::WritesNotPermitted(_))
        ));
        assert_eq!(device.transport().write_packets(), 0);
        assert_eq!(device.transport().received().len(), 2);

        let usb = UsbIdentity {
            bcd_device: None,
            ..reference_usb()
        };
        let device = Device::connect(SimulatedKeyboard::reference(), usb).unwrap();
        assert!(matches!(device.write_access(), WriteAccess::ReadOnly(_)));
    }

    #[test]
    fn v2_magic_is_refused_outright() {
        let mut keyboard = SimulatedKeyboard::reference();
        let mut block = *keyboard.capabilities();
        block[..2].copy_from_slice(&[0xaa, 0x55]);
        keyboard.set_capabilities(block);
        assert!(matches!(
            Device::connect(keyboard, reference_usb()),
            Err(DeviceError::IdentityMismatch(_))
        ));
    }

    #[test]
    fn reads_return_the_keyboard_state() {
        let mut device = connect();
        let block = device.read_config(Profile::One).unwrap();
        assert_eq!(block.mode(), Some(Mode::SpectrumCycle));
        let map = device.read_colour_map(Profile::One).unwrap();
        assert_eq!(map.len(), COLOUR_SLOTS);
        assert!(map[..18].iter().all(|&c| c == Rgb::new(255, 0, 0)));
        assert!(map[18..].iter().all(|&c| c == Rgb::default()));
        assert!(device.transport().violations().is_empty());
    }

    #[test]
    fn key_presses_and_stale_replies_are_skipped() {
        let mut keyboard = SimulatedKeyboard::reference();
        let mut key_press = vec![0u8; 16];
        key_press[0] = 0x01;
        keyboard.queue_foreign_report(key_press);
        // A vendor reply to a different request, as left by a timed-out one.
        keyboard.queue_foreign_report(Request::read_config(Profile::Three).encode().to_vec());
        let device = Device::connect(keyboard, reference_usb()).unwrap();
        assert_eq!(device.capabilities().active_profile(), Some(Profile::One));
    }

    #[test]
    fn lost_reply_times_out_and_the_next_exchange_recovers() {
        let mut device = connect();
        device.link.transport.drop_replies(1);
        assert!(matches!(
            device.read_config(Profile::Two),
            Err(DeviceError::Timeout)
        ));
        assert!(device.read_config(Profile::Two).is_ok());
    }

    #[test]
    fn committed_transaction_applies_its_writes() {
        let mut device = connect();
        let mut transaction = device.transaction().unwrap();
        transaction
            .set(Profile::Two, Setting::Mode(Mode::Static))
            .unwrap();
        transaction
            .write_colours(Profile::Two, 0, &[Rgb::new(0, 0, 255); 20])
            .unwrap();
        transaction.commit().unwrap();

        let keyboard = device.transport();
        assert_eq!(keyboard.config(Profile::Two)[0], Mode::Static.id());
        assert_eq!(keyboard.colour(Profile::Two, 19), Some(Rgb::new(0, 0, 255)));
        assert_eq!(keyboard.committed_transactions(), 1);
        assert!(keyboard.violations().is_empty());
        assert_eq!(device.write_packets(), 3);
        assert_eq!(device.transactions(), 1);
    }

    #[test]
    fn dropped_transaction_still_sends_end() {
        let mut device = connect();
        {
            let mut transaction = device.transaction().unwrap();
            transaction
                .set(Profile::One, Setting::Brightness(2))
                .unwrap();
        }
        let keyboard = device.transport();
        assert!(!keyboard.in_transaction());
        assert_eq!(keyboard.committed_transactions(), 1);
        assert!(keyboard.violations().is_empty());
    }

    #[test]
    fn failed_write_still_sends_end() {
        let mut device = connect();
        let result = (|| {
            let mut transaction = device.transaction()?;
            transaction.link.transport.drop_replies(1);
            transaction.set(Profile::One, Setting::Speed(3))?;
            transaction.commit()
        })();
        assert!(matches!(result, Err(DeviceError::Timeout)));
        let keyboard = device.transport();
        assert!(!keyboard.in_transaction());
        assert!(keyboard.violations().is_empty());
    }

    #[test]
    fn lost_begin_reply_still_sends_end() {
        let mut device = connect();
        device.link.transport.drop_replies(1);
        assert!(matches!(device.transaction(), Err(DeviceError::Timeout)));
        let keyboard = device.transport();
        assert!(!keyboard.in_transaction());
        assert!(keyboard.violations().is_empty());
    }

    #[test]
    fn panic_inside_a_transaction_still_sends_end() {
        let mut device = connect();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _transaction = device.transaction().unwrap();
            panic!("caller bug");
        }));
        assert!(result.is_err());
        assert!(!device.transport().in_transaction());
    }

    #[test]
    fn disconnection_is_remembered() {
        let mut device = connect();
        device.link.transport.unplug();
        assert!(matches!(
            device.read_config(Profile::One),
            Err(DeviceError::Disconnected)
        ));
        device.link.transport.plug_in();
        // The device object stays dead; the caller must rediscover (AV-014).
        assert!(matches!(
            device.read_config(Profile::One),
            Err(DeviceError::Disconnected)
        ));
    }

    #[test]
    fn unplug_mid_transaction_is_reported() {
        let mut device = connect();
        let result = (|| {
            let mut transaction = device.transaction()?;
            transaction.link.transport.unplug();
            transaction.set(Profile::One, Setting::Speed(1))
        })();
        assert!(matches!(result, Err(DeviceError::Disconnected)));
        assert_eq!(device.transport().violations(), &[] as &[Violation]);
    }
}
